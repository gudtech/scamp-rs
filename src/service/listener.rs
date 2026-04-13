//! TLS listener, connection handling, and request dispatch.
//!
//! Matches Perl Transport::BEEPish::Server.pm.

use anyhow::{anyhow, Result};
use log;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_native_tls::native_tls;
use tokio_native_tls::TlsAcceptor;

use super::announce;
use super::handler::{ActionHandlerFn, ActionInfo, RegisteredAction, ScampReply, ScampRequest};
use crate::transport::beepish::proto::{
    EnvelopeFormat, FlexInt, MessageType, Packet, PacketHeader, PacketType, ParseResult,
    DATA_CHUNK_SIZE,
};

/// SCAMP service that listens for incoming connections and dispatches requests.
pub struct ScampService {
    name: String,
    identity: String,
    sector: String,
    envelopes: Vec<String>,
    actions: HashMap<String, RegisteredAction>,
    listener: Option<TcpListener>,
    tls_acceptor: Option<TlsAcceptor>,
    address: Option<SocketAddr>,
    key_pem: Option<Vec<u8>>,
    cert_pem: Option<Vec<u8>>,
    announce_ip: Option<String>,
}

impl ScampService {
    pub fn new(name: &str, sector: &str) -> Self {
        let random_bytes: [u8; 18] = rand::random();
        let identity_suffix =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, random_bytes);

        ScampService {
            name: name.to_string(),
            identity: format!("{}:{}", name, identity_suffix),
            sector: sector.to_string(),
            envelopes: vec!["json".to_string()],
            actions: HashMap::new(),
            listener: None,
            tls_acceptor: None,
            address: None,
            key_pem: None,
            cert_pem: None,
            announce_ip: None,
        }
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn address(&self) -> Option<SocketAddr> {
        self.address
    }

    pub fn uri(&self) -> Option<String> {
        self.address.map(|addr| {
            let ip = self
                .announce_ip
                .as_deref()
                .unwrap_or(&addr.ip().to_string())
                .to_string();
            format!("beepish+tls://{}:{}", ip, addr.port())
        })
    }

    pub fn set_announce_ip(&mut self, ip: &str) {
        self.announce_ip = Some(ip.to_string());
    }

    /// Snapshot of registered action info for use by the announcer task.
    pub fn actions_snapshot(&self) -> Vec<ActionInfo> {
        self.actions.values().map(ActionInfo::from).collect()
    }

    pub fn register<F, Fut>(&mut self, action: &str, version: i32, handler: F)
    where
        F: Fn(ScampRequest) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ScampReply> + Send + 'static,
    {
        let key = format!("{}.v{}", action.to_lowercase(), version);
        let handler: ActionHandlerFn = Arc::new(move |req| Box::pin(handler(req)));
        self.actions.insert(
            key,
            RegisteredAction {
                name: action.to_string(),
                version,
                handler,
            },
        );
    }

    pub async fn bind_pem(&mut self, key_pem: &[u8], cert_pem: &[u8]) -> Result<()> {
        self.key_pem = Some(key_pem.to_vec());
        self.cert_pem = Some(cert_pem.to_vec());

        let key = native_tls::Identity::from_pkcs8(cert_pem, key_pem)?;
        let tls = native_tls::TlsAcceptor::builder(key).build()?;

        // Perl Server.pm:27-29
        let first_port: u16 = 30100;
        let last_port: u16 = 30399;
        let bind_tries: u32 = 20;

        let mut listener = None;
        for _ in 0..bind_tries {
            let port = first_port + (rand::random::<u16>() % (last_port - first_port + 1));
            let addr: SocketAddr = format!("0.0.0.0:{}", port).parse()?;
            match TcpListener::bind(addr).await {
                Ok(l) => {
                    listener = Some(l);
                    break;
                }
                Err(_) => continue,
            }
        }

        let listener =
            listener.ok_or_else(|| anyhow!("Failed to bind after {} tries", bind_tries))?;
        let addr = listener.local_addr()?;
        log::info!("Bound to beepish+tls://{}:{}", addr.ip(), addr.port());

        self.listener = Some(listener);
        self.tls_acceptor = Some(TlsAcceptor::from(tls));
        self.address = Some(addr);
        Ok(())
    }

    /// Build a signed announcement packet (uncompressed bytes).
    /// Perl Announcer.pm:122-204
    pub fn build_announcement_packet(&self, active: bool) -> Result<Vec<u8>> {
        let key_pem = self.key_pem.as_ref().ok_or_else(|| anyhow!("No key"))?;
        let cert_pem = self.cert_pem.as_ref().ok_or_else(|| anyhow!("No cert"))?;
        let uri = self.uri().ok_or_else(|| anyhow!("Not bound"))?;
        let action_infos: Vec<ActionInfo> =
            self.actions.values().map(ActionInfo::from).collect();

        announce::build_announcement_packet(
            &self.identity,
            &self.sector,
            &self.envelopes,
            &uri,
            &action_infos,
            key_pem,
            cert_pem,
            1,  // weight
            5,  // interval_secs
            active,
        )
    }

    pub async fn run(self) -> Result<()> {
        let listener = self
            .listener
            .ok_or_else(|| anyhow!("Not bound — call bind_pem() first"))?;
        let tls_acceptor = self
            .tls_acceptor
            .ok_or_else(|| anyhow!("Not bound — call bind_pem() first"))?;
        let actions = Arc::new(self.actions);

        loop {
            let (stream, peer_addr) = listener.accept().await?;
            stream.set_nodelay(true)?;
            let tls_acceptor = tls_acceptor.clone();
            let actions = actions.clone();

            tokio::spawn(async move {
                match tls_acceptor.accept(stream).await {
                    Ok(tls_stream) => {
                        log::debug!("Accepted connection from {}", peer_addr);
                        handle_connection(tls_stream, actions).await;
                    }
                    Err(e) => {
                        log::error!("TLS accept failed from {}: {}", peer_addr, e);
                    }
                }
            });
        }
    }
}

struct IncomingRequest {
    header: PacketHeader,
    body: Vec<u8>,
    received: usize,
}

type ServerWriter =
    Arc<Mutex<tokio::io::WriteHalf<tokio_native_tls::TlsStream<tokio::net::TcpStream>>>>;

async fn handle_connection(
    tls_stream: tokio_native_tls::TlsStream<tokio::net::TcpStream>,
    actions: Arc<HashMap<String, RegisteredAction>>,
) {
    let (reader, writer) = tokio::io::split(tls_stream);
    let writer: ServerWriter = Arc::new(Mutex::new(writer));
    let mut reader = BufReader::new(reader);
    let mut incoming: HashMap<u64, IncomingRequest> = HashMap::new();
    let mut next_incoming_msg_no: u64 = 0;
    let next_outgoing_msg_no = AtomicU64::new(0);

    loop {
        let buf = match reader.fill_buf().await {
            Ok(buf) if buf.is_empty() => break,
            Ok(buf) => buf,
            Err(e) => {
                log::debug!("Read error: {}", e);
                break;
            }
        };

        let mut consumed = 0;
        while consumed < buf.len() {
            match Packet::parse(&buf[consumed..]) {
                ParseResult::TooShort | ParseResult::NeedBytes { .. } => break,
                ParseResult::Drop { bytes_used } => consumed += bytes_used,
                ParseResult::Success {
                    packet,
                    bytes_used,
                } => {
                    consumed += bytes_used;
                    route_packet(
                        packet,
                        &mut incoming,
                        &mut next_incoming_msg_no,
                        &next_outgoing_msg_no,
                        &writer,
                        &actions,
                    )
                    .await;
                }
                ParseResult::Fatal(err) => {
                    log::error!("Fatal protocol error: {}", err);
                    return;
                }
            }
        }
        reader.consume(consumed);
    }
}

async fn route_packet(
    packet: Packet,
    incoming: &mut HashMap<u64, IncomingRequest>,
    next_incoming_msg_no: &mut u64,
    next_outgoing_msg_no: &AtomicU64,
    writer: &ServerWriter,
    actions: &Arc<HashMap<String, RegisteredAction>>,
) {
    match packet.packet_type {
        PacketType::Header => {
            if packet.msg_no != *next_incoming_msg_no {
                log::error!("Out of sequence: expected {} got {}", *next_incoming_msg_no, packet.msg_no);
                return;
            }
            *next_incoming_msg_no += 1;
            if let Some(header) = packet.packet_header {
                incoming.insert(packet.msg_no, IncomingRequest { header, body: Vec::new(), received: 0 });
            }
        }
        PacketType::Data => {
            if let Some(msg) = incoming.get_mut(&packet.msg_no) {
                msg.body.extend_from_slice(&packet.body);
                msg.received += packet.body.len();
                let ack = Packet { packet_type: PacketType::Ack, msg_no: packet.msg_no, packet_header: None, body: msg.received.to_string().into_bytes() };
                let mut w = writer.lock().await;
                let _ = ack.write(&mut *w).await;
                let _ = w.flush().await;
            }
        }
        PacketType::Eof => {
            if let Some(msg) = incoming.remove(&packet.msg_no) {
                dispatch_and_reply(msg, next_outgoing_msg_no, writer, actions).await;
            }
        }
        PacketType::Txerr => {
            // JS connection.js:229 — empty/"0" TXERR body is invalid
            let body_str = String::from_utf8_lossy(&packet.body);
            if body_str.is_empty() || body_str == "0" {
                log::error!("TXERR with empty/zero body for msgno {}", packet.msg_no);
                return;
            }
            incoming.remove(&packet.msg_no);
        }
        PacketType::Ack => {}
        PacketType::Ping => {
            let pong = Packet { packet_type: PacketType::Pong, msg_no: packet.msg_no, packet_header: None, body: vec![] };
            let mut w = writer.lock().await;
            let _ = pong.write(&mut *w).await;
            let _ = w.flush().await;
        }
        PacketType::Pong => {}
    }
}

async fn dispatch_and_reply(
    msg: IncomingRequest,
    next_outgoing_msg_no: &AtomicU64,
    writer: &ServerWriter,
    actions: &Arc<HashMap<String, RegisteredAction>>,
) {
    let request_id = msg.header.request_id;
    let action_key = format!("{}.v{}", msg.header.action.to_lowercase(), msg.header.version);

    let request = ScampRequest {
        action: msg.header.action, version: msg.header.version,
        envelope: msg.header.envelope, request_id: msg.header.request_id,
        client_id: msg.header.client_id, ticket: msg.header.ticket,
        identifying_token: msg.header.identifying_token, body: msg.body,
    };

    let reply = if let Some(registered) = actions.get(&action_key) {
        (registered.handler)(request).await
    } else {
        ScampReply::error(format!("No such action: {}", action_key), "not_found".to_string())
    };

    let reply_msg_no = next_outgoing_msg_no.fetch_add(1, Ordering::Relaxed);
    let reply_header = PacketHeader {
        action: String::new(), envelope: EnvelopeFormat::Json,
        error: reply.error, error_code: reply.error_code,
        request_id, // Perl Server.pm:66
        client_id: FlexInt(0), ticket: String::new(), identifying_token: String::new(),
        message_type: MessageType::Reply, // Perl Server.pm:68
        version: 0,
    };

    let mut w = writer.lock().await;
    let _ = Packet { packet_type: PacketType::Header, msg_no: reply_msg_no, packet_header: Some(reply_header), body: vec![] }.write(&mut *w).await;
    let mut offset = 0;
    while offset < reply.body.len() {
        let end = (offset + DATA_CHUNK_SIZE).min(reply.body.len());
        let _ = Packet { packet_type: PacketType::Data, msg_no: reply_msg_no, packet_header: None, body: reply.body[offset..end].to_vec() }.write(&mut *w).await;
        offset = end;
    }
    let _ = Packet { packet_type: PacketType::Eof, msg_no: reply_msg_no, packet_header: None, body: vec![] }.write(&mut *w).await;
    let _ = w.flush().await;
}

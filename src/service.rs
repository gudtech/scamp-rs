//! SCAMP service: accepts incoming connections, dispatches requests to handlers.
//!
//! This implements the server side of the SCAMP protocol, matching
//! Perl Transport::BEEPish::Server and JS actor/service.js.

use anyhow::{anyhow, Result};
use log;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_native_tls::native_tls::{self, Identity};
use tokio_native_tls::TlsAcceptor;

use crate::config::Config;
use crate::transport::beepish::proto::{
    EnvelopeFormat, FlexInt, MessageType, Packet, PacketHeader, PacketType, ParseResult,
    DATA_CHUNK_SIZE,
};

/// A request received by the service.
pub struct ScampRequest {
    pub action: String,
    pub version: i32,
    pub envelope: EnvelopeFormat,
    pub request_id: FlexInt,
    pub client_id: FlexInt,
    pub ticket: String,
    pub identifying_token: String,
    pub body: Vec<u8>,
}

/// A response to send back.
pub struct ScampReply {
    pub body: Vec<u8>,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

impl ScampReply {
    pub fn ok(body: Vec<u8>) -> Self {
        ScampReply {
            body,
            error: None,
            error_code: None,
        }
    }

    pub fn error(message: String, code: String) -> Self {
        ScampReply {
            body: vec![],
            error: Some(message),
            error_code: Some(code),
        }
    }
}

/// Handler function type for registered actions.
pub type ActionHandlerFn =
    Arc<dyn Fn(ScampRequest) -> std::pin::Pin<Box<dyn std::future::Future<Output = ScampReply> + Send>> + Send + Sync>;

/// A registered action with its handler.
struct RegisteredAction {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    version: i32,
    handler: ActionHandlerFn,
}

/// SCAMP service that listens for incoming connections and dispatches requests.
pub struct ScampService {
    name: String,
    identity: String,
    sector: String,
    actions: HashMap<String, RegisteredAction>,
    listener: Option<TcpListener>,
    tls_acceptor: Option<TlsAcceptor>,
    address: Option<SocketAddr>,
}

impl ScampService {
    /// Create a new service with the given name and sector.
    /// The TLS identity must be provided (PEM key + cert in PKCS12 format).
    pub fn new(name: &str, sector: &str) -> Self {
        let random_bytes: [u8; 18] = rand::random();
        let identity_suffix = base64_encode(&random_bytes);
        let identity = format!("{}:{}", name, identity_suffix);

        ScampService {
            name: name.to_string(),
            identity,
            sector: sector.to_string(),
            actions: HashMap::new(),
            listener: None,
            tls_acceptor: None,
            address: None,
        }
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn address(&self) -> Option<SocketAddr> {
        self.address
    }

    pub fn uri(&self) -> Option<String> {
        self.address
            .map(|addr| format!("beepish+tls://{}:{}", addr.ip(), addr.port()))
    }

    /// Register an action handler.
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

    /// Bind to a TLS port. Uses PKCS12 identity (key + cert).
    pub async fn bind(&mut self, pkcs12_der: &[u8], password: &str) -> Result<()> {
        let identity = Identity::from_pkcs12(pkcs12_der, password)?;
        let tls = native_tls::TlsAcceptor::builder(identity).build()?;
        let tls_acceptor = TlsAcceptor::from(tls);

        // Try to bind to a port in the configured range
        // Perl Server.pm: first_port=30100, last_port=30399, bind_tries=20
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

        let listener = listener.ok_or_else(|| anyhow!("Failed to bind after {} tries", bind_tries))?;
        let addr = listener.local_addr()?;
        log::info!("Bound to beepish+tls://{}:{}", addr.ip(), addr.port());

        self.listener = Some(listener);
        self.tls_acceptor = Some(tls_acceptor);
        self.address = Some(addr);

        Ok(())
    }

    /// Bind using PEM key and certificate files (more common in SCAMP).
    pub async fn bind_pem(&mut self, key_pem: &[u8], cert_pem: &[u8]) -> Result<()> {
        // native-tls requires PKCS12, so we need to convert PEM to PKCS12
        // For now, use the openssl crate or manual conversion
        // TODO: switch to rustls which handles PEM natively
        let key = native_tls::Identity::from_pkcs8(cert_pem, key_pem)?;
        let tls = native_tls::TlsAcceptor::builder(key).build()?;
        let tls_acceptor = TlsAcceptor::from(tls);

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

        let listener = listener.ok_or_else(|| anyhow!("Failed to bind after {} tries", bind_tries))?;
        let addr = listener.local_addr()?;
        log::info!("Bound to beepish+tls://{}:{}", addr.ip(), addr.port());

        self.listener = Some(listener);
        self.tls_acceptor = Some(tls_acceptor);
        self.address = Some(addr);

        Ok(())
    }

    /// Run the service accept loop. This is the main entry point.
    pub async fn run(self) -> Result<()> {
        let listener = self
            .listener
            .ok_or_else(|| anyhow!("Service not bound — call bind() first"))?;
        let tls_acceptor = self
            .tls_acceptor
            .ok_or_else(|| anyhow!("Service not bound — call bind() first"))?;

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
                        Self::handle_connection(tls_stream, actions).await;
                    }
                    Err(e) => {
                        log::error!("TLS accept failed from {}: {}", peer_addr, e);
                    }
                }
            });
        }
    }

    /// Handle a single connection: read packets, assemble messages, dispatch.
    async fn handle_connection(
        tls_stream: tokio_native_tls::TlsStream<tokio::net::TcpStream>,
        actions: Arc<HashMap<String, RegisteredAction>>,
    ) {
        let (reader, writer) = tokio::io::split(tls_stream);
        let writer = Arc::new(Mutex::new(writer));
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

            let mut bytes_consumed = 0;

            while bytes_consumed < buf.len() {
                match Packet::parse(&buf[bytes_consumed..]) {
                    ParseResult::TooShort | ParseResult::NeedBytes { .. } => break,
                    ParseResult::Drop { bytes_used } => {
                        bytes_consumed += bytes_used;
                    }
                    ParseResult::Success {
                        packet,
                        bytes_used,
                    } => {
                        bytes_consumed += bytes_used;
                        Self::route_server_packet(
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

            reader.consume(bytes_consumed);
        }
    }

    /// Route a packet on the server side — assemble request messages, dispatch to handlers.
    async fn route_server_packet(
        packet: Packet,
        incoming: &mut HashMap<u64, IncomingRequest>,
        next_incoming_msg_no: &mut u64,
        next_outgoing_msg_no: &AtomicU64,
        writer: &Arc<Mutex<tokio::io::WriteHalf<tokio_native_tls::TlsStream<tokio::net::TcpStream>>>>,
        actions: &Arc<HashMap<String, RegisteredAction>>,
    ) {
        match packet.packet_type {
            PacketType::Header => {
                if packet.msg_no != *next_incoming_msg_no {
                    log::error!(
                        "Out of sequence: expected {} got {}",
                        *next_incoming_msg_no,
                        packet.msg_no
                    );
                    return;
                }
                *next_incoming_msg_no += 1;

                if let Some(header) = packet.packet_header {
                    incoming.insert(
                        packet.msg_no,
                        IncomingRequest {
                            header,
                            body: Vec::new(),
                            received: 0,
                        },
                    );
                }
            }

            PacketType::Data => {
                if let Some(msg) = incoming.get_mut(&packet.msg_no) {
                    msg.body.extend_from_slice(&packet.body);
                    msg.received += packet.body.len();

                    // Send ACK
                    let ack_body = msg.received.to_string();
                    let ack = Packet {
                        packet_type: PacketType::Ack,
                        msg_no: packet.msg_no,
                        packet_header: None,
                        body: ack_body.into_bytes(),
                    };
                    let mut w = writer.lock().await;
                    let _ = ack.write(&mut *w).await;
                    let _ = w.flush().await;
                }
            }

            PacketType::Eof => {
                if let Some(msg) = incoming.remove(&packet.msg_no) {
                    // Dispatch to handler
                    let request_id = msg.header.request_id;
                    let action_key = format!(
                        "{}.v{}",
                        msg.header.action.to_lowercase(),
                        msg.header.version
                    );

                    let request = ScampRequest {
                        action: msg.header.action,
                        version: msg.header.version,
                        envelope: msg.header.envelope,
                        request_id: msg.header.request_id,
                        client_id: msg.header.client_id,
                        ticket: msg.header.ticket,
                        identifying_token: msg.header.identifying_token,
                        body: msg.body,
                    };

                    let reply = if let Some(registered) = actions.get(&action_key) {
                        (registered.handler)(request).await
                    } else {
                        ScampReply::error(
                            format!("No such action: {}", action_key),
                            "not_found".to_string(),
                        )
                    };

                    // Send reply
                    let reply_msg_no = next_outgoing_msg_no.fetch_add(1, Ordering::Relaxed);

                    let reply_header = PacketHeader {
                        action: String::new(),
                        envelope: EnvelopeFormat::Json,
                        error: reply.error,
                        error_code: reply.error_code,
                        request_id, // Copy from request (Perl Server.pm:66)
                        client_id: FlexInt(0),
                        ticket: String::new(),
                        identifying_token: String::new(),
                        message_type: MessageType::Reply, // "reply" not "response" (Perl Server.pm:68)
                        version: 0,
                    };

                    let mut w = writer.lock().await;

                    // HEADER
                    let header_pkt = Packet {
                        packet_type: PacketType::Header,
                        msg_no: reply_msg_no,
                        packet_header: Some(reply_header),
                        body: vec![],
                    };
                    let _ = header_pkt.write(&mut *w).await;

                    // DATA chunks
                    let mut offset = 0;
                    while offset < reply.body.len() {
                        let end = (offset + DATA_CHUNK_SIZE).min(reply.body.len());
                        let data_pkt = Packet {
                            packet_type: PacketType::Data,
                            msg_no: reply_msg_no,
                            packet_header: None,
                            body: reply.body[offset..end].to_vec(),
                        };
                        let _ = data_pkt.write(&mut *w).await;
                        offset = end;
                    }

                    // EOF (empty body)
                    let eof_pkt = Packet {
                        packet_type: PacketType::Eof,
                        msg_no: reply_msg_no,
                        packet_header: None,
                        body: vec![],
                    };
                    let _ = eof_pkt.write(&mut *w).await;
                    let _ = w.flush().await;
                }
            }

            PacketType::Txerr => {
                incoming.remove(&packet.msg_no);
            }

            PacketType::Ack => {
                // ACK for outgoing messages — not needed for server since we
                // send replies all at once (no streaming yet)
            }

            PacketType::Ping => {
                let pong = Packet {
                    packet_type: PacketType::Pong,
                    msg_no: packet.msg_no,
                    packet_header: None,
                    body: vec![],
                };
                let mut w = writer.lock().await;
                let _ = pong.write(&mut *w).await;
                let _ = w.flush().await;
            }

            PacketType::Pong => {}
        }
    }
}

struct IncomingRequest {
    header: PacketHeader,
    body: Vec<u8>,
    received: usize,
}

/// Simple base64 encoding for identity generation
fn base64_encode(data: &[u8]) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut encoder = Base64Encoder::new(&mut buf);
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap();
    }
    String::from_utf8(buf).unwrap()
}

/// Minimal base64 encoder (avoids adding a dependency just for this)
struct Base64Encoder<W: std::io::Write> {
    writer: W,
    buf: [u8; 3],
    buf_len: usize,
}

const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

impl<W: std::io::Write> Base64Encoder<W> {
    fn new(writer: W) -> Self {
        Base64Encoder {
            writer,
            buf: [0; 3],
            buf_len: 0,
        }
    }

    fn flush_buf(&mut self) -> std::io::Result<()> {
        if self.buf_len == 0 {
            return Ok(());
        }
        let b = &self.buf;
        let out = match self.buf_len {
            3 => [
                B64_CHARS[(b[0] >> 2) as usize],
                B64_CHARS[((b[0] & 0x03) << 4 | b[1] >> 4) as usize],
                B64_CHARS[((b[1] & 0x0f) << 2 | b[2] >> 6) as usize],
                B64_CHARS[(b[2] & 0x3f) as usize],
            ],
            2 => [
                B64_CHARS[(b[0] >> 2) as usize],
                B64_CHARS[((b[0] & 0x03) << 4 | b[1] >> 4) as usize],
                B64_CHARS[((b[1] & 0x0f) << 2) as usize],
                b'=',
            ],
            1 => [
                B64_CHARS[(b[0] >> 2) as usize],
                B64_CHARS[((b[0] & 0x03) << 4) as usize],
                b'=',
                b'=',
            ],
            _ => unreachable!(),
        };
        self.writer.write_all(&out)?;
        self.buf_len = 0;
        Ok(())
    }

    fn finish(mut self) -> std::io::Result<W> {
        self.flush_buf()?;
        Ok(self.writer)
    }
}

impl<W: std::io::Write> std::io::Write for Base64Encoder<W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let mut i = 0;
        while i < data.len() {
            self.buf[self.buf_len] = data[i];
            self.buf_len += 1;
            i += 1;
            if self.buf_len == 3 {
                self.flush_buf()?;
            }
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

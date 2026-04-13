//! Connection pooling and request sending.

use anyhow::{anyhow, Context, Result};
use log;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex, Notify};
use tokio::time::timeout;
use tokio_native_tls::{native_tls, TlsConnector};

use super::reader;
use crate::config::Config;
use crate::discovery::ServiceInfo;
use crate::transport::beepish::proto::{
    EnvelopeFormat, FlexInt, MessageType, Packet, PacketHeader, PacketType, DATA_CHUNK_SIZE,
};

/// Default per-request (RPC) timeout — Perl ServiceInfo.pm:257
pub const DEFAULT_RPC_TIMEOUT_SECS: u64 = 75;

/// Default client connection idle timeout — Perl Client.pm:40
pub const DEFAULT_CLIENT_TIMEOUT_SECS: u64 = 90;

/// Default server connection idle timeout — Perl Server.pm:58
#[allow(dead_code)]
pub const DEFAULT_SERVER_TIMEOUT_SECS: u64 = 120;

/// Send-side flow control watermark — JS connection.js:4
const FLOW_CONTROL_WATERMARK: u64 = 65536;

/// A SCAMP response received from a service.
#[derive(Debug)]
pub struct ScampResponse {
    pub header: PacketHeader,
    pub body: Vec<u8>,
    pub error: Option<String>,
}

/// High-level SCAMP client that manages connections to services.
pub struct BeepishClient {
    config: Config,
    connections: Arc<Mutex<HashMap<String, Arc<ConnectionHandle>>>>,
}

/// Handle to a single connection with background reader/writer tasks.
pub struct ConnectionHandle {
    writer_tx: mpsc::Sender<Packet>,
    pending: Arc<Mutex<HashMap<i64, oneshot::Sender<ScampResponse>>>>,
    outgoing: reader::OutgoingMap,
    ack_notify: Arc<Notify>,
    next_request_id: AtomicI64,
    next_outgoing_msg_no: AtomicU64,
    closed: Arc<AtomicBool>,
    reader_handle: tokio::task::JoinHandle<()>,
    writer_handle: tokio::task::JoinHandle<()>,
}

impl BeepishClient {
    pub fn new(config: &Config) -> Self {
        BeepishClient {
            config: config.clone(),
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn get_connection(&self, service_info: &ServiceInfo) -> Result<Arc<ConnectionHandle>> {
        let mut connections = self.connections.lock().await;
        if let Some(conn) = connections.get(&service_info.uri) {
            if !conn.closed.load(Ordering::Relaxed) {
                return Ok(conn.clone());
            }
            connections.remove(&service_info.uri);
        }
        let handle = Arc::new(
            ConnectionHandle::connect(&self.config, service_info, service_info.fingerprint.as_deref()).await?
        );
        connections.insert(service_info.uri.clone(), handle.clone());
        Ok(handle)
    }

    pub async fn request(
        &self, service_info: &ServiceInfo, action: &str, version: i32,
        envelope: EnvelopeFormat, ticket: &str, client_id: i64,
        body: Vec<u8>, timeout_secs: Option<u64>,
    ) -> Result<ScampResponse> {
        let conn = self.get_connection(service_info).await?;
        let dur = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_RPC_TIMEOUT_SECS));
        conn.send_request(action, version, envelope, ticket, client_id, body, dur).await
    }
}

impl ConnectionHandle {
    /// Set up reader/writer tasks over any async stream.
    /// Used by `connect()` after TLS and by tests with in-memory streams.
    pub(crate) fn from_stream(
        stream: impl AsyncRead + AsyncWrite + Unpin + Send + 'static,
    ) -> Self {
        let (read_half, write_half) = tokio::io::split(stream);
        let (writer_tx, writer_rx) = mpsc::channel::<Packet>(256);
        let pending: Arc<Mutex<HashMap<i64, oneshot::Sender<ScampResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let outgoing: reader::OutgoingMap = Arc::new(Mutex::new(HashMap::new()));
        let ack_notify = Arc::new(Notify::new());

        let writer_handle = tokio::spawn(writer_task(write_half, writer_rx));
        let reader_pending = pending.clone();
        let reader_writer_tx = writer_tx.clone();
        let reader_closed = closed.clone();
        let reader_outgoing = outgoing.clone();
        let reader_ack_notify = ack_notify.clone();
        let reader_handle = tokio::spawn(async move {
            reader::reader_task(
                read_half, reader_pending, reader_writer_tx,
                reader_outgoing, reader_ack_notify,
            ).await;
            // D12: set closed flag when reader exits
            reader_closed.store(true, Ordering::Relaxed);
        });

        ConnectionHandle {
            writer_tx, pending, outgoing, ack_notify,
            next_request_id: AtomicI64::new(1),    // Perl Client.pm:33
            next_outgoing_msg_no: AtomicU64::new(0), // All impls start at 0
            closed,
            reader_handle, writer_handle,
        }
    }

    /// Connect with TLS fingerprint verification (Perl Connection.pm:61-68).
    async fn connect(
        _config: &Config, service_info: &ServiceInfo, expected_fingerprint: Option<&str>,
    ) -> Result<Self> {
        let tls = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()?;
        let connector = TlsConnector::from(tls);
        let addr = service_info.socket_addr()
            .map_err(|e| anyhow::anyhow!("Bad service URI: {}", e))?;

        let stream = timeout(Duration::from_secs(30), TcpStream::connect(addr))
            .await.context("TCP connection timed out")?.context("Failed to connect")?;
        stream.set_nodelay(true)?;

        let tls_stream = timeout(Duration::from_secs(30), connector.connect(&addr.ip().to_string(), stream))
            .await.context("TLS handshake timed out")?.context("TLS handshake failed")?;

        // Fingerprint verification before any packets (natural corking)
        if let Some(expected_fp) = expected_fingerprint {
            let peer_cert = tls_stream.get_ref().peer_certificate()
                .context("Failed to get peer certificate")?
                .ok_or_else(|| anyhow!("Peer did not present a certificate"))?;
            let peer_der = peer_cert.to_der().context("Failed to get peer certificate DER")?;
            let actual_fp = crate::crypto::cert_sha1_fingerprint(&peer_der);
            if actual_fp != expected_fp {
                return Err(anyhow!("CERTIFICATE MISMATCH! Announced {} got {}", expected_fp, actual_fp));
            }
            log::debug!("Certificate fingerprint verified: {}", actual_fp);
        }

        Ok(Self::from_stream(tls_stream))
    }

    pub async fn send_request(
        &self, action: &str, version: i32, envelope: EnvelopeFormat,
        ticket: &str, client_id: i64, body: Vec<u8>, timeout_duration: Duration,
    ) -> Result<ScampResponse> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(anyhow!("Connection is closed"));
        }

        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let msg_no = self.next_outgoing_msg_no.fetch_add(1, Ordering::Relaxed);
        let (response_tx, response_rx) = oneshot::channel();

        { self.pending.lock().await.insert(request_id, response_tx); }

        let header = PacketHeader {
            action: action.to_string(), envelope, error: None, error_code: None,
            error_data: None, request_id: FlexInt(request_id), client_id: FlexInt(client_id),
            ticket: ticket.to_string(), identifying_token: String::new(),
            message_type: MessageType::Request, version,
        };

        // Register outgoing message for ACK tracking (D5)
        { self.outgoing.lock().await.insert(msg_no, reader::OutgoingState::default()); }

        // HEADER
        if self.writer_tx.send(Packet { packet_type: PacketType::Header, msg_no, packet_header: Some(header), body: vec![] }).await.is_err() {
            self.pending.lock().await.remove(&request_id);
            self.outgoing.lock().await.remove(&msg_no);
            return Err(anyhow!("Connection closed while sending header"));
        }

        // DATA chunks — track bytes sent for ACK validation
        let mut offset = 0;
        while offset < body.len() {
            // D5b: Flow control — wait if unacked bytes >= watermark
            // JS connection.js:298: pause when sent-acked >= 65536
            loop {
                if self.closed.load(Ordering::Relaxed) {
                    self.pending.lock().await.remove(&request_id);
                    self.outgoing.lock().await.remove(&msg_no);
                    return Err(anyhow!("Connection closed during flow control wait"));
                }
                let over_watermark = {
                    let out = self.outgoing.lock().await;
                    out.get(&msg_no).map_or(false, |s| {
                        s.sent.saturating_sub(s.acknowledged) >= FLOW_CONTROL_WATERMARK
                    })
                };
                if !over_watermark { break; }
                self.ack_notify.notified().await;
            }

            let end = (offset + DATA_CHUNK_SIZE).min(body.len());
            let chunk_len = (end - offset) as u64;
            if self.writer_tx.send(Packet { packet_type: PacketType::Data, msg_no, packet_header: None, body: body[offset..end].to_vec() }).await.is_err() {
                self.pending.lock().await.remove(&request_id);
                self.outgoing.lock().await.remove(&msg_no);
                return Err(anyhow!("Connection closed while sending data"));
            }
            { self.outgoing.lock().await.get_mut(&msg_no).map(|s| s.sent += chunk_len); }
            offset = end;
        }

        // EOF (empty body — Perl Connection.pm:162)
        if self.writer_tx.send(Packet { packet_type: PacketType::Eof, msg_no, packet_header: None, body: vec![] }).await.is_err() {
            self.pending.lock().await.remove(&request_id);
            self.outgoing.lock().await.remove(&msg_no);
            return Err(anyhow!("Connection closed while sending EOF"));
        }

        // Clean up outgoing state after message is fully sent
        self.outgoing.lock().await.remove(&msg_no);

        match timeout(timeout_duration, response_rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(anyhow!("Connection lost while waiting for response")),
            Err(_) => {
                self.pending.lock().await.remove(&request_id);
                Err(anyhow!("Request timed out after {:?}", timeout_duration))
            }
        }
    }
}

impl Drop for ConnectionHandle {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
        self.reader_handle.abort();
        self.writer_handle.abort();
    }
}

async fn writer_task(mut writer: impl AsyncWrite + Unpin, mut rx: mpsc::Receiver<Packet>) {
    while let Some(packet) = rx.recv().await {
        if let Err(e) = packet.write(&mut writer).await {
            log::error!("Error writing packet: {}", e);
            break;
        }
        if let Err(e) = writer.flush().await {
            log::error!("Error flushing writer: {}", e);
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::server_connection;
    use crate::test_helpers::echo_actions;

    #[tokio::test]
    async fn test_client_echo() {
        let (client_stream, server_stream) = tokio::io::duplex(65536);
        let actions = echo_actions();
        let _server = tokio::spawn(
            server_connection::handle_connection(server_stream, actions, None),
        );

        let conn = ConnectionHandle::from_stream(client_stream);
        let resp = conn
            .send_request(
                "echo", 1, EnvelopeFormat::Json, "", 0,
                b"hello from client".to_vec(), Duration::from_secs(5),
            )
            .await
            .unwrap();

        assert!(resp.error.is_none());
        assert_eq!(resp.body, b"hello from client");
        assert_eq!(resp.header.message_type, MessageType::Reply);
        assert_eq!(resp.header.request_id.0, 1);
    }

    #[tokio::test]
    async fn test_client_unknown_action_error() {
        let (client_stream, server_stream) = tokio::io::duplex(65536);
        let _server = tokio::spawn(
            server_connection::handle_connection(server_stream, echo_actions(), None),
        );

        let conn = ConnectionHandle::from_stream(client_stream);
        let resp = conn
            .send_request(
                "nonexistent", 1, EnvelopeFormat::Json, "", 0,
                b"{}".to_vec(), Duration::from_secs(5),
            )
            .await
            .unwrap();

        assert!(resp.header.error.as_ref().unwrap().contains("No such action"));
        assert_eq!(resp.header.error_code.as_deref(), Some("not_found"));
    }

    #[tokio::test]
    async fn test_client_large_body() {
        let (client_stream, server_stream) = tokio::io::duplex(65536);
        let _server = tokio::spawn(
            server_connection::handle_connection(server_stream, echo_actions(), None),
        );

        let body = vec![0xABu8; 5000];
        let conn = ConnectionHandle::from_stream(client_stream);
        let resp = conn
            .send_request(
                "echo", 1, EnvelopeFormat::Json, "", 0,
                body.clone(), Duration::from_secs(5),
            )
            .await
            .unwrap();

        assert!(resp.error.is_none());
        assert_eq!(resp.body.len(), 5000);
        assert_eq!(resp.body, body);
    }

    #[tokio::test]
    async fn test_client_request_timeout() {
        // Server that accepts but never responds
        let (client_stream, _server_stream) = tokio::io::duplex(65536);
        let conn = ConnectionHandle::from_stream(client_stream);

        let result = conn
            .send_request(
                "echo", 1, EnvelopeFormat::Json, "", 0,
                b"{}".to_vec(), Duration::from_millis(100),
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("timed out"), "Expected timeout error, got: {}", err);
    }
}

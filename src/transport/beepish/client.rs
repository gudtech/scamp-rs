use anyhow::{anyhow, Context, Result};
use log;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::timeout;
use tokio_native_tls::{native_tls, TlsConnector, TlsStream};

use super::proto::{
    EnvelopeFormat, FlexInt, MessageType, Packet, PacketHeader, PacketType, ParseResult,
    DATA_CHUNK_SIZE,
};
use crate::config::Config;
use crate::discovery::ServiceInfo;

const MAX_FLOW: usize = 65536;
const DEFAULT_TIMEOUT_SECS: u64 = 75;

/// A SCAMP response received from a service.
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

/// Handle to a single connection. Allows sending requests and awaiting responses.
/// The reader and writer tasks run in the background.
pub struct ConnectionHandle {
    writer_tx: mpsc::Sender<Packet>,
    pending: Arc<Mutex<HashMap<i64, oneshot::Sender<ScampResponse>>>>,
    next_request_id: AtomicI64,
    next_outgoing_msg_no: AtomicU64,
    closed: AtomicBool,
    reader_handle: tokio::task::JoinHandle<()>,
    writer_handle: tokio::task::JoinHandle<()>,
}

/// An in-progress incoming message being assembled from packets.
struct IncomingMessage {
    header: PacketHeader,
    body: Vec<u8>,
    received: usize,
    acked: usize,
}

impl BeepishClient {
    pub fn new(config: &Config) -> Self {
        BeepishClient {
            config: config.clone(),
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get or create a connection to the given service.
    pub async fn get_connection(
        &self,
        service_info: &ServiceInfo,
    ) -> Result<Arc<ConnectionHandle>> {
        let mut connections = self.connections.lock().await;

        // Check if we have an open connection
        if let Some(conn) = connections.get(&service_info.uri) {
            if !conn.closed.load(Ordering::Relaxed) {
                return Ok(conn.clone());
            }
            // Connection is closed, remove it
            connections.remove(&service_info.uri);
        }

        // Create new connection
        let handle = ConnectionHandle::connect(&self.config, service_info).await?;
        let handle = Arc::new(handle);
        connections.insert(service_info.uri.clone(), handle.clone());

        // Connection cleanup happens lazily: when get_connection() finds a
        // closed connection, it removes it and creates a new one.

        Ok(handle)
    }

    /// Send a request and await the response.
    pub async fn request(
        &self,
        service_info: &ServiceInfo,
        action: &str,
        version: i32,
        envelope: EnvelopeFormat,
        ticket: &str,
        client_id: i64,
        body: Vec<u8>,
        timeout_secs: Option<u64>,
    ) -> Result<ScampResponse> {
        let conn = self.get_connection(service_info).await?;
        let timeout_duration =
            Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));

        conn.send_request(action, version, envelope, ticket, client_id, body, timeout_duration)
            .await
    }
}

impl ConnectionHandle {
    /// Establish a new TLS connection to a SCAMP service.
    async fn connect(config: &Config, service_info: &ServiceInfo) -> Result<Self> {
        let tls = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()?;
        let connector = TlsConnector::from(tls);

        let addr = service_info.socket_addr();

        let stream = timeout(Duration::from_secs(30), TcpStream::connect(addr))
            .await
            .context("TCP connection timed out")?
            .context("Failed to connect")?;

        stream.set_nodelay(true)?;

        let tls_stream = timeout(
            Duration::from_secs(30),
            connector.connect(&addr.ip().to_string(), stream),
        )
        .await
        .context("TLS handshake timed out")?
        .context("TLS handshake failed")?;

        // No protocol-level handshake — go straight to packet I/O.
        // Perl/Go/JS all begin packet exchange immediately after TLS.

        let (reader, writer) = tokio::io::split(tls_stream);

        // Channel for serialized writes — all packets go through here
        let (writer_tx, writer_rx) = mpsc::channel::<Packet>(256);

        // Pending requests map — keyed by request_id
        let pending: Arc<Mutex<HashMap<i64, oneshot::Sender<ScampResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Spawn writer task
        let writer_handle = tokio::spawn(Self::writer_task(writer, writer_rx));

        // Spawn reader task
        let pending_clone = pending.clone();
        let writer_tx_clone = writer_tx.clone();
        let reader_handle =
            tokio::spawn(Self::reader_task(reader, pending_clone, writer_tx_clone));

        Ok(ConnectionHandle {
            writer_tx,
            pending,
            next_request_id: AtomicI64::new(1), // Perl starts at 1
            next_outgoing_msg_no: AtomicU64::new(0), // All impls start at 0
            closed: AtomicBool::new(false),
            reader_handle,
            writer_handle,
        })
    }

    /// Send a request and await the response with timeout.
    pub async fn send_request(
        &self,
        action: &str,
        version: i32,
        envelope: EnvelopeFormat,
        ticket: &str,
        client_id: i64,
        body: Vec<u8>,
        timeout_duration: Duration,
    ) -> Result<ScampResponse> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(anyhow!("Connection is closed"));
        }

        // Allocate request_id (sequential, starting from 1 — matches Perl)
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);

        // Allocate outgoing message number (starting from 0)
        let msg_no = self.next_outgoing_msg_no.fetch_add(1, Ordering::Relaxed);

        // Create response channel
        let (response_tx, response_rx) = oneshot::channel();

        // Insert into pending map
        {
            let mut pending = self.pending.lock().await;
            pending.insert(request_id, response_tx);
        }

        // Build header
        let header = PacketHeader {
            action: action.to_string(),
            envelope,
            error: None,
            error_code: None,
            request_id: FlexInt(request_id),
            client_id: FlexInt(client_id),
            ticket: ticket.to_string(),
            identifying_token: String::new(),
            message_type: MessageType::Request,
            version,
        };

        // Send HEADER packet
        let header_packet = Packet {
            packet_type: PacketType::Header,
            msg_no,
            packet_header: Some(header),
            body: vec![],
        };

        if self.writer_tx.send(header_packet).await.is_err() {
            self.remove_pending(request_id).await;
            return Err(anyhow!("Connection closed while sending header"));
        }

        // Send DATA packets (chunked at DATA_CHUNK_SIZE)
        let mut offset = 0;
        while offset < body.len() {
            let end = (offset + DATA_CHUNK_SIZE).min(body.len());
            let chunk = body[offset..end].to_vec();
            let data_packet = Packet {
                packet_type: PacketType::Data,
                msg_no,
                packet_header: None,
                body: chunk,
            };
            if self.writer_tx.send(data_packet).await.is_err() {
                self.remove_pending(request_id).await;
                return Err(anyhow!("Connection closed while sending data"));
            }
            offset = end;
        }

        // Send EOF packet (empty body — required by Perl)
        let eof_packet = Packet {
            packet_type: PacketType::Eof,
            msg_no,
            packet_header: None,
            body: vec![],
        };
        if self.writer_tx.send(eof_packet).await.is_err() {
            self.remove_pending(request_id).await;
            return Err(anyhow!("Connection closed while sending EOF"));
        }

        // Await response with timeout
        match timeout(timeout_duration, response_rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                // Channel closed — connection lost
                Err(anyhow!("Connection lost while waiting for response"))
            }
            Err(_) => {
                // Timeout
                self.remove_pending(request_id).await;
                Err(anyhow!(
                    "Request timed out after {:?}",
                    timeout_duration
                ))
            }
        }
    }

    async fn remove_pending(&self, request_id: i64) {
        let mut pending = self.pending.lock().await;
        pending.remove(&request_id);
    }

    /// Writer task: receives packets from the channel and writes them to the TLS stream.
    async fn writer_task(
        mut writer: WriteHalf<TlsStream<TcpStream>>,
        mut rx: mpsc::Receiver<Packet>,
    ) {
        while let Some(packet) = rx.recv().await {
            if let Err(e) = packet.write(&mut writer).await {
                log::error!("Error writing packet: {}", e);
                break;
            }
            // Flush after each packet to ensure timely delivery
            if let Err(e) = writer.flush().await {
                log::error!("Error flushing writer: {}", e);
                break;
            }
        }
    }

    /// Reader task: reads packets from the TLS stream, assembles messages,
    /// and delivers completed responses to pending requesters.
    async fn reader_task(
        reader: ReadHalf<TlsStream<TcpStream>>,
        pending: Arc<Mutex<HashMap<i64, oneshot::Sender<ScampResponse>>>>,
        writer_tx: mpsc::Sender<Packet>,
    ) {
        let mut reader = BufReader::new(reader);
        let mut incoming: HashMap<u64, IncomingMessage> = HashMap::new();
        let mut next_incoming_msg_no: u64 = 0; // Starts at 0 — matches all implementations

        loop {
            // Fill the buffer
            let buf = match reader.fill_buf().await {
                Ok(buf) if buf.is_empty() => break, // EOF
                Ok(buf) => buf,
                Err(e) => {
                    log::error!("Read error: {}", e);
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

                        Self::route_packet(
                            packet,
                            &mut incoming,
                            &mut next_incoming_msg_no,
                            &pending,
                            &writer_tx,
                        )
                        .await;
                    }

                    ParseResult::Fatal(err) => {
                        log::error!("Fatal protocol error: {}", err);
                        // Clean up all pending requests
                        let mut pend = pending.lock().await;
                        for (_, tx) in pend.drain() {
                            let _ = tx.send(ScampResponse {
                                header: PacketHeader::default(),
                                body: vec![],
                                error: Some(format!("Protocol error: {err}")),
                            });
                        }
                        return;
                    }
                }
            }

            reader.consume(bytes_consumed);
        }

        // Connection closed — notify all pending requests
        let mut pend = pending.lock().await;
        for (_, tx) in pend.drain() {
            let _ = tx.send(ScampResponse {
                header: PacketHeader::default(),
                body: vec![],
                error: Some("Connection lost".to_string()),
            });
        }
    }

    /// Route a single packet to the appropriate handler.
    /// Implements message assembly from HEADER → DATA* → EOF/TXERR.
    async fn route_packet(
        packet: Packet,
        incoming: &mut HashMap<u64, IncomingMessage>,
        next_incoming_msg_no: &mut u64,
        pending: &Arc<Mutex<HashMap<i64, oneshot::Sender<ScampResponse>>>>,
        writer_tx: &mpsc::Sender<Packet>,
    ) {
        match packet.packet_type {
            PacketType::Header => {
                // Validate sequential message number (Perl Connection.pm:140)
                if packet.msg_no != *next_incoming_msg_no {
                    log::error!(
                        "Out of sequence message: expected {} got {}",
                        *next_incoming_msg_no,
                        packet.msg_no
                    );
                    return;
                }
                *next_incoming_msg_no += 1;

                let header = match packet.packet_header {
                    Some(h) => h,
                    None => return,
                };

                incoming.insert(
                    packet.msg_no,
                    IncomingMessage {
                        header,
                        body: Vec::new(),
                        received: 0,
                        acked: 0,
                    },
                );
            }

            PacketType::Data => {
                let msg = match incoming.get_mut(&packet.msg_no) {
                    Some(m) => m,
                    None => {
                        log::error!("Received DATA with no active message for msgno {}", packet.msg_no);
                        return;
                    }
                };

                if packet.body.is_empty() {
                    return; // JS skips empty DATA (connection.js:202)
                }

                msg.body.extend_from_slice(&packet.body);
                msg.received += packet.body.len();

                // Send ACK with cumulative bytes received (decimal string)
                // Perl Connection.pm:153, JS connection.js:208
                let ack_body = msg.received.to_string();
                msg.acked = msg.received;
                let ack_packet = Packet {
                    packet_type: PacketType::Ack,
                    msg_no: packet.msg_no,
                    packet_header: None,
                    body: ack_body.into_bytes(),
                };
                let _ = writer_tx.send(ack_packet).await;
            }

            PacketType::Eof => {
                // EOF body must be empty (Perl Connection.pm:162)
                if !packet.body.is_empty() {
                    log::error!("EOF packet must be empty");
                    return;
                }

                let msg = match incoming.remove(&packet.msg_no) {
                    Some(m) => m,
                    None => {
                        log::error!("Received EOF with no active message for msgno {}", packet.msg_no);
                        return;
                    }
                };

                // Deliver to pending requester by request_id
                let request_id = msg.header.request_id.0;
                let mut pend = pending.lock().await;
                if let Some(tx) = pend.remove(&request_id) {
                    let _ = tx.send(ScampResponse {
                        header: msg.header,
                        body: msg.body,
                        error: None,
                    });
                }
            }

            PacketType::Txerr => {
                let msg = match incoming.remove(&packet.msg_no) {
                    Some(m) => m,
                    None => {
                        log::error!("Received TXERR with no active message for msgno {}", packet.msg_no);
                        return;
                    }
                };

                let error_text = String::from_utf8_lossy(&packet.body).to_string();

                // Deliver as error to pending requester
                let request_id = msg.header.request_id.0;
                let mut pend = pending.lock().await;
                if let Some(tx) = pend.remove(&request_id) {
                    let _ = tx.send(ScampResponse {
                        header: msg.header,
                        body: msg.body,
                        error: Some(error_text),
                    });
                }
            }

            PacketType::Ack => {
                // ACK for outgoing messages — flow control
                // For now we don't implement send-side flow control pause/resume
                // since we send all data immediately. This will be needed for
                // large message streaming.
                // Perl Connection.pm:177-183 validates the ACK value
            }

            PacketType::Ping => {
                // Respond with PONG (JS connection.js:257)
                let pong = Packet {
                    packet_type: PacketType::Pong,
                    msg_no: packet.msg_no,
                    packet_header: None,
                    body: vec![],
                };
                let _ = writer_tx.send(pong).await;
            }

            PacketType::Pong => {
                // Heartbeat response received — would reset heartbeat timer
                // Not implemented yet since heartbeat is disabled by default
            }
        }
    }
}

impl Default for PacketHeader {
    fn default() -> Self {
        PacketHeader {
            action: String::new(),
            envelope: EnvelopeFormat::Json,
            error: None,
            error_code: None,
            request_id: FlexInt(0),
            client_id: FlexInt(0),
            ticket: String::new(),
            identifying_token: String::new(),
            message_type: MessageType::Request,
            version: 1,
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

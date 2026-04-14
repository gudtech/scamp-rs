//! Server-side connection handling and request dispatch.
//! Matches Perl Transport::BEEPish::Server.pm connection handling.

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;

use super::handler::{RegisteredAction, ScampReply, ScampRequest};
use super::server_reply::send_reply;
use crate::auth::authz::AuthzChecker;
use crate::transport::beepish::proto::{Packet, PacketHeader, PacketType, ParseResult, MAX_PACKET_SIZE};

/// Server connection idle timeout — Perl Server.pm:58, Connection.pm:131-135
const DEFAULT_SERVER_TIMEOUT_SECS: u64 = 120;

struct IncomingRequest {
    header: PacketHeader,
    body: Vec<u8>,
    received: usize,
}

/// Writer half, boxed for testability (allows in-memory streams in tests).
pub(crate) type ServerWriter = Arc<Mutex<Box<dyn AsyncWrite + Unpin + Send>>>;

/// Tracks bytes sent/acknowledged for outgoing replies.
/// Used for ACK validation (monotonic, not past end) and is_busy detection.
/// Server-side does NOT pause on watermark — matches Perl, which sends replies unbounded.
#[derive(Debug, Default)]
pub(crate) struct OutgoingReplyState {
    pub(crate) sent: u64,
    pub(crate) acknowledged: u64,
}

/// Handle a single server connection: read packets, dispatch requests, send replies.
/// Accepts any async stream for testability (production passes TLS streams).
pub(crate) async fn handle_connection(
    stream: impl AsyncRead + AsyncWrite + Unpin + Send + 'static,
    actions: Arc<HashMap<String, RegisteredAction>>,
    authz: Option<Arc<AuthzChecker>>,
) {
    let (mut reader, writer) = tokio::io::split(stream);
    let writer: ServerWriter = Arc::new(Mutex::new(Box::new(writer)));
    let mut buf = Vec::with_capacity(8192);
    let mut incoming: HashMap<u64, IncomingRequest> = HashMap::new();
    let mut outgoing: HashMap<u64, OutgoingReplyState> = HashMap::new();
    let mut next_incoming_msg_no: u64 = 0;
    let next_outgoing_msg_no = AtomicU64::new(0);

    loop {
        // Read more data from the stream.
        // Perl Connection.pm:131-135 — no timeout when busy, idle timeout otherwise.
        let is_busy = !incoming.is_empty() || !outgoing.is_empty();
        let mut tmp = [0u8; 4096];
        let n = if is_busy {
            match reader.read(&mut tmp).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    log::debug!("Read error: {}", e);
                    break;
                }
            }
        } else {
            let idle_timeout = std::time::Duration::from_secs(DEFAULT_SERVER_TIMEOUT_SECS);
            match tokio::time::timeout(idle_timeout, reader.read(&mut tmp)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => n,
                Ok(Err(e)) => {
                    log::debug!("Read error: {}", e);
                    break;
                }
                Err(_) => {
                    log::debug!("Idle timeout ({}s)", idle_timeout.as_secs());
                    break;
                }
            }
        };
        buf.extend_from_slice(&tmp[..n]);
        // M1: Cap buffer to prevent OOM from adversarial streams
        if buf.len() > MAX_PACKET_SIZE + 100 {
            log::error!("Read buffer exceeded max packet size, closing");
            return;
        }

        // Parse all complete packets from the buffer.
        let mut consumed = 0;
        while consumed < buf.len() {
            match Packet::parse(&buf[consumed..]) {
                ParseResult::TooShort | ParseResult::NeedBytes { .. } => break,
                ParseResult::Drop { bytes_used } => consumed += bytes_used,
                ParseResult::Success { packet, bytes_used } => {
                    consumed += bytes_used;
                    let ok = route_packet(
                        packet,
                        &mut incoming,
                        &mut outgoing,
                        &mut next_incoming_msg_no,
                        &next_outgoing_msg_no,
                        &writer,
                        &actions,
                        &authz,
                    )
                    .await;
                    if !ok {
                        return;
                    }
                }
                ParseResult::Fatal(err) => {
                    log::error!("Fatal protocol error: {}", err);
                    return;
                }
            }
        }
        buf.drain(..consumed);
    }
}

/// Returns false if the connection should be closed.
async fn route_packet(
    packet: Packet,
    incoming: &mut HashMap<u64, IncomingRequest>,
    outgoing: &mut HashMap<u64, OutgoingReplyState>,
    next_incoming_msg_no: &mut u64,
    next_outgoing_msg_no: &AtomicU64,
    writer: &ServerWriter,
    actions: &Arc<HashMap<String, RegisteredAction>>,
    authz: &Option<Arc<AuthzChecker>>,
) -> bool {
    match packet.packet_type {
        PacketType::Header => {
            // Perl Connection.pm:140 — out-of-sequence HEADER is fatal, close connection
            if packet.msg_no != *next_incoming_msg_no {
                log::error!("Out of sequence: expected {} got {}, closing", *next_incoming_msg_no, packet.msg_no);
                return false;
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
                let ack = Packet {
                    packet_type: PacketType::Ack,
                    msg_no: packet.msg_no,
                    packet_header: None,
                    body: msg.received.to_string().into_bytes(),
                };
                let mut w = writer.lock().await;
                if let Err(e) = ack.write(&mut *w).await {
                    log::error!("Failed to write ACK: {}", e);
                    return false;
                }
                if let Err(e) = w.flush().await {
                    log::error!("Flush failed: {}", e);
                }
            }
        }
        PacketType::Eof => {
            // Perl Connection.pm:162 — EOF body must be empty
            if !packet.body.is_empty() {
                log::error!("EOF packet has non-empty body ({} bytes)", packet.body.len());
                return true;
            }
            if let Some(msg) = incoming.remove(&packet.msg_no) {
                dispatch_and_reply(msg, next_outgoing_msg_no, outgoing, writer, actions, authz).await;
            }
        }
        PacketType::Txerr => {
            // JS connection.js:229 — empty/"0" TXERR body is invalid
            let body_str = String::from_utf8_lossy(&packet.body);
            if body_str.is_empty() || body_str == "0" {
                log::error!("TXERR with empty/zero body for msgno {}", packet.msg_no);
                return true; // protocol error but not fatal to connection
            }
            incoming.remove(&packet.msg_no);
        }
        PacketType::Ack => {
            // Perl Connection.pm:177-183 — validate ACK value
            let body_str = String::from_utf8_lossy(&packet.body);
            let ack_val: u64 = match body_str.parse() {
                Ok(v) if v > 0 => v,
                _ => {
                    log::error!("Malformed ACK body: {:?}", body_str);
                    return true;
                }
            };
            if let Some(state) = outgoing.get_mut(&packet.msg_no) {
                if ack_val <= state.acknowledged {
                    log::error!("ACK pointer moved backward: {} <= {}", ack_val, state.acknowledged);
                    return true;
                }
                if ack_val > state.sent {
                    log::error!("ACK pointer past end: {} > sent {}", ack_val, state.sent);
                    return true;
                }
                state.acknowledged = ack_val;
            }
        }
        PacketType::Ping => {
            let pong = Packet {
                packet_type: PacketType::Pong,
                msg_no: packet.msg_no,
                packet_header: None,
                body: vec![],
            };
            let mut w = writer.lock().await;
            if let Err(e) = pong.write(&mut *w).await {
                log::error!("Failed to write PONG: {}", e);
                return false;
            }
            if let Err(e) = w.flush().await {
                log::error!("Flush failed: {}", e);
            }
        }
        PacketType::Pong => {}
    }
    true
}

async fn dispatch_and_reply(
    msg: IncomingRequest,
    next_outgoing_msg_no: &AtomicU64,
    outgoing: &mut HashMap<u64, OutgoingReplyState>,
    writer: &ServerWriter,
    actions: &Arc<HashMap<String, RegisteredAction>>,
    authz: &Option<Arc<AuthzChecker>>,
) {
    let request_id = msg.header.request_id;
    let action_key = format!("{}.v{}", msg.header.action.to_lowercase(), msg.header.version);

    // C1: Check ticket privileges before dispatch — JS ticket.js:71-93
    // Skip for actions with "noauth" flag, or if no AuthzChecker configured.
    let noauth = actions
        .get(&action_key)
        .map(|a| a.flags.iter().any(|f| f == "noauth"))
        .unwrap_or(false);
    if let Some(checker) = authz {
        if !noauth && !msg.header.ticket.is_empty() {
            if let Err(e) = checker.check_access(&msg.header.action, &msg.header.ticket).await {
                log::warn!("Authorization denied for {}: {}", action_key, e);
                let reply = ScampReply::error(e.to_string(), "unauthorized".to_string());
                send_reply(reply, request_id, next_outgoing_msg_no, outgoing, writer).await;
                return;
            }
        }
    }

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
        ScampReply::error(format!("No such action: {}", action_key), "not_found".to_string())
    };

    send_reply(reply, request_id, next_outgoing_msg_no, outgoing, writer).await;
}

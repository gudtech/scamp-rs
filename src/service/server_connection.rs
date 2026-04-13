//! Server-side connection handling and request dispatch.
//! Matches Perl Transport::BEEPish::Server.pm connection handling.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use super::handler::{RegisteredAction, ScampReply, ScampRequest};
use crate::auth::authz::AuthzChecker;
use crate::transport::beepish::proto::{
    EnvelopeFormat, FlexInt, MessageType, Packet, PacketHeader, PacketType, ParseResult,
    DATA_CHUNK_SIZE,
};

/// Server connection idle timeout — Perl Server.pm:58, Connection.pm:131-135
const DEFAULT_SERVER_TIMEOUT_SECS: u64 = 120;

struct IncomingRequest {
    header: PacketHeader,
    body: Vec<u8>,
    received: usize,
}

/// Writer half, boxed for testability (allows in-memory streams in tests).
type ServerWriter = Arc<Mutex<Box<dyn AsyncWrite + Unpin + Send>>>;

/// Tracks bytes sent/acknowledged for outgoing replies (D5 flow control).
#[derive(Debug, Default)]
struct OutgoingReplyState {
    sent: u64,
    acknowledged: u64,
}

/// Handle a single server connection: read packets, dispatch requests, send replies.
/// Accepts any async stream for testability (production passes TLS streams).
pub(crate) async fn handle_connection(
    stream: impl AsyncRead + AsyncWrite + Unpin + Send + 'static,
    actions: Arc<HashMap<String, RegisteredAction>>,
    authz: Option<Arc<AuthzChecker>>,
) {
    let (reader, writer) = tokio::io::split(stream);
    let writer: ServerWriter = Arc::new(Mutex::new(Box::new(writer)));
    let mut reader = BufReader::new(reader);
    let mut incoming: HashMap<u64, IncomingRequest> = HashMap::new();
    let mut outgoing: HashMap<u64, OutgoingReplyState> = HashMap::new();
    let mut next_incoming_msg_no: u64 = 0;
    let next_outgoing_msg_no = AtomicU64::new(0);

    loop {
        // Perl Connection.pm:131-135 — _adj_timeout: no timeout when busy,
        // configured timeout when idle. Server default: 120s.
        let is_busy = !incoming.is_empty() || !outgoing.is_empty();
        let buf = if is_busy {
            match reader.fill_buf().await {
                Ok(buf) if buf.is_empty() => break,
                Ok(buf) => buf,
                Err(e) => {
                    log::debug!("Read error: {}", e);
                    break;
                }
            }
        } else {
            let idle_timeout = std::time::Duration::from_secs(DEFAULT_SERVER_TIMEOUT_SECS);
            match tokio::time::timeout(idle_timeout, reader.fill_buf()).await {
                Ok(Ok(buf)) if buf.is_empty() => break,
                Ok(Ok(buf)) => buf,
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

        let mut consumed = 0;
        while consumed < buf.len() {
            match Packet::parse(&buf[consumed..]) {
                ParseResult::TooShort | ParseResult::NeedBytes { .. } => break,
                ParseResult::Drop { bytes_used } => consumed += bytes_used,
                ParseResult::Success { packet, bytes_used } => {
                    consumed += bytes_used;
                    route_packet(
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
    outgoing: &mut HashMap<u64, OutgoingReplyState>,
    next_incoming_msg_no: &mut u64,
    next_outgoing_msg_no: &AtomicU64,
    writer: &ServerWriter,
    actions: &Arc<HashMap<String, RegisteredAction>>,
    authz: &Option<Arc<AuthzChecker>>,
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
                let ack = Packet {
                    packet_type: PacketType::Ack,
                    msg_no: packet.msg_no,
                    packet_header: None,
                    body: msg.received.to_string().into_bytes(),
                };
                let mut w = writer.lock().await;
                if let Err(e) = ack.write(&mut *w).await {
                    log::error!("Failed to write ACK: {}", e);
                    return;
                }
                if let Err(e) = w.flush().await {
                    log::error!("Flush failed: {}", e);
                }
            }
        }
        PacketType::Eof => {
            if let Some(msg) = incoming.remove(&packet.msg_no) {
                dispatch_and_reply(msg, next_outgoing_msg_no, outgoing, writer, actions, authz)
                    .await;
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
        PacketType::Ack => {
            // Perl Connection.pm:177-183 — validate ACK value
            let body_str = String::from_utf8_lossy(&packet.body);
            let ack_val: u64 = match body_str.parse() {
                Ok(v) if v > 0 => v,
                _ => {
                    log::error!("Malformed ACK body: {:?}", body_str);
                    return;
                }
            };
            if let Some(state) = outgoing.get_mut(&packet.msg_no) {
                if ack_val <= state.acknowledged {
                    log::error!(
                        "ACK pointer moved backward: {} <= {}",
                        ack_val,
                        state.acknowledged
                    );
                    return;
                }
                if ack_val > state.sent {
                    log::error!("ACK pointer past end: {} > sent {}", ack_val, state.sent);
                    return;
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
                return;
            }
            if let Err(e) = w.flush().await {
                log::error!("Flush failed: {}", e);
            }
        }
        PacketType::Pong => {}
    }
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
    let action_key = format!(
        "{}.v{}",
        msg.header.action.to_lowercase(),
        msg.header.version
    );

    // C1: Check ticket privileges before dispatch — JS ticket.js:71-93
    // Skip for actions with "noauth" flag, or if no AuthzChecker configured.
    let noauth = actions
        .get(&action_key)
        .map(|a| a.flags.iter().any(|f| f == "noauth"))
        .unwrap_or(false);
    if let Some(checker) = authz {
        if !noauth && !msg.header.ticket.is_empty() {
            if let Err(e) = checker
                .check_access(&msg.header.action, &msg.header.ticket)
                .await
            {
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
        ScampReply::error(
            format!("No such action: {}", action_key),
            "not_found".to_string(),
        )
    };

    send_reply(reply, request_id, next_outgoing_msg_no, outgoing, writer).await;
}

async fn send_reply(
    reply: ScampReply,
    request_id: FlexInt,
    next_outgoing_msg_no: &AtomicU64,
    outgoing: &mut HashMap<u64, OutgoingReplyState>,
    writer: &ServerWriter,
) {
    let reply_msg_no = next_outgoing_msg_no.fetch_add(1, Ordering::Relaxed);
    let reply_header = PacketHeader {
        action: String::new(),
        envelope: EnvelopeFormat::Json,
        error: reply.error,
        error_code: reply.error_code,
        error_data: None,
        request_id,
        client_id: FlexInt(0),
        ticket: String::new(),
        identifying_token: String::new(),
        message_type: MessageType::Reply,
        version: 0,
    };

    outgoing.insert(reply_msg_no, OutgoingReplyState::default());

    let mut w = writer.lock().await;
    let header_pkt = Packet {
        packet_type: PacketType::Header,
        msg_no: reply_msg_no,
        packet_header: Some(reply_header),
        body: vec![],
    };
    if let Err(e) = header_pkt.write(&mut *w).await {
        log::error!("Failed to write reply HEADER: {}", e);
        outgoing.remove(&reply_msg_no);
        return;
    }

    let mut offset = 0;
    while offset < reply.body.len() {
        let end = (offset + DATA_CHUNK_SIZE).min(reply.body.len());
        let chunk_len = (end - offset) as u64;
        let data_pkt = Packet {
            packet_type: PacketType::Data,
            msg_no: reply_msg_no,
            packet_header: None,
            body: reply.body[offset..end].to_vec(),
        };
        if let Err(e) = data_pkt.write(&mut *w).await {
            log::error!("Failed to write reply DATA: {}", e);
            outgoing.remove(&reply_msg_no);
            return;
        }
        if let Some(state) = outgoing.get_mut(&reply_msg_no) {
            state.sent += chunk_len;
        }
        offset = end;
    }

    let eof_pkt = Packet {
        packet_type: PacketType::Eof,
        msg_no: reply_msg_no,
        packet_header: None,
        body: vec![],
    };
    if let Err(e) = eof_pkt.write(&mut *w).await {
        log::error!("Failed to write reply EOF: {}", e);
    }
    if let Err(e) = w.flush().await {
        log::error!("Reply flush failed: {}", e);
    }

    outgoing.remove(&reply_msg_no);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{echo_actions, parse_all_packets, write_request};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Send a single request and collect all response packets.
    async fn roundtrip(
        actions: Arc<HashMap<String, RegisteredAction>>,
        action: &str,
        version: i32,
        body: &[u8],
    ) -> Vec<Packet> {
        let (client, server) = tokio::io::duplex(65536);
        let server_handle = tokio::spawn(handle_connection(server, actions, None));
        let (mut client_read, mut client_write) = tokio::io::split(client);

        write_request(&mut client_write, 0, action, version, 1, body).await;
        client_write.shutdown().await.unwrap();

        let mut response_data = Vec::new();
        client_read.read_to_end(&mut response_data).await.unwrap();
        server_handle.await.unwrap();
        parse_all_packets(&response_data)
    }

    #[tokio::test]
    async fn test_echo_roundtrip() {
        let packets = roundtrip(echo_actions(), "echo", 1, b"hello world").await;

        let reply_hdr = packets
            .iter()
            .find(|p| p.packet_type == PacketType::Header)
            .expect("no reply HEADER");
        let header = reply_hdr.packet_header.as_ref().unwrap();
        assert_eq!(header.message_type, MessageType::Reply);
        assert_eq!(header.request_id.0, 1);
        assert!(header.error.is_none());

        let reply_body: Vec<u8> = packets
            .iter()
            .filter(|p| p.packet_type == PacketType::Data && p.msg_no == reply_hdr.msg_no)
            .flat_map(|p| p.body.iter().cloned())
            .collect();
        assert_eq!(reply_body, b"hello world");

        assert!(packets
            .iter()
            .any(|p| p.packet_type == PacketType::Eof && p.msg_no == reply_hdr.msg_no));
    }

    #[tokio::test]
    async fn test_error_for_unknown_action() {
        let packets = roundtrip(echo_actions(), "nonexistent", 1, b"{}").await;

        let reply_hdr = packets
            .iter()
            .find(|p| p.packet_type == PacketType::Header)
            .expect("no reply HEADER");
        let header = reply_hdr.packet_header.as_ref().unwrap();
        assert_eq!(header.message_type, MessageType::Reply);
        assert!(header.error.as_ref().unwrap().contains("No such action"));
        assert_eq!(header.error_code.as_deref(), Some("not_found"));
    }

    #[tokio::test]
    async fn test_empty_body_request() {
        let packets = roundtrip(echo_actions(), "echo", 1, b"").await;

        let reply_hdr = packets
            .iter()
            .find(|p| p.packet_type == PacketType::Header)
            .expect("no reply HEADER");
        assert!(reply_hdr.packet_header.as_ref().unwrap().error.is_none());

        let data_count = packets
            .iter()
            .filter(|p| p.packet_type == PacketType::Data && p.msg_no == reply_hdr.msg_no)
            .count();
        assert_eq!(data_count, 0, "echo of empty body should produce no DATA");
    }

    #[tokio::test]
    async fn test_ping_pong() {
        let (client, server) = tokio::io::duplex(65536);
        let server_handle = tokio::spawn(handle_connection(server, echo_actions(), None));
        let (mut client_read, mut client_write) = tokio::io::split(client);

        Packet {
            packet_type: PacketType::Ping,
            msg_no: 0,
            packet_header: None,
            body: vec![],
        }
        .write(&mut client_write)
        .await
        .unwrap();
        client_write.shutdown().await.unwrap();

        let mut response_data = Vec::new();
        client_read.read_to_end(&mut response_data).await.unwrap();
        server_handle.await.unwrap();

        let packets = parse_all_packets(&response_data);
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].packet_type, PacketType::Pong);
        assert_eq!(packets[0].msg_no, 0);
    }

    #[tokio::test]
    async fn test_ack_sent_on_data() {
        let packets = roundtrip(echo_actions(), "echo", 1, b"hello").await;

        let ack = packets
            .iter()
            .find(|p| p.packet_type == PacketType::Ack && p.msg_no == 0)
            .expect("no ACK for request DATA");
        let ack_val: usize = String::from_utf8_lossy(&ack.body).parse().unwrap();
        assert_eq!(ack_val, 5, "ACK should be cumulative bytes (5 for 'hello')");
    }

    #[tokio::test]
    async fn test_multi_chunk_roundtrip() {
        let body = vec![0x42u8; 5000];
        let packets = roundtrip(echo_actions(), "echo", 1, &body).await;

        let reply_hdr = packets
            .iter()
            .find(|p| p.packet_type == PacketType::Header)
            .expect("no reply HEADER");

        let reply_body: Vec<u8> = packets
            .iter()
            .filter(|p| p.packet_type == PacketType::Data && p.msg_no == reply_hdr.msg_no)
            .flat_map(|p| p.body.iter().cloned())
            .collect();
        assert_eq!(reply_body.len(), 5000);
        assert_eq!(reply_body, body);

        // 5000 / 2048 = 3 chunks (2048 + 2048 + 904)
        let data_count = packets
            .iter()
            .filter(|p| p.packet_type == PacketType::Data && p.msg_no == reply_hdr.msg_no)
            .count();
        assert_eq!(data_count, 3);
    }
}

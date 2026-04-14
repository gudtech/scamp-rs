//! Client-side reader task: reads packets, assembles messages, delivers responses.
//!
//! Implements inbound message assembly from HEADER → DATA* → EOF/TXERR.
//! Matches Perl Connection.pm _packet() and JS connection.js _onpacket.

use log;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::{mpsc, oneshot, Mutex, Notify};

use super::ScampResponse;
use crate::transport::beepish::proto::{Packet, PacketHeader, PacketType, ParseResult, MAX_PACKET_SIZE};

/// In-progress incoming message being assembled from packets.
struct IncomingMessage {
    header: PacketHeader,
    body: Vec<u8>,
    received: usize,
}

/// Tracks bytes sent/acknowledged for an outgoing message (D5 flow control).
/// Perl Connection.pm:177-183
#[derive(Debug, Default)]
pub(super) struct OutgoingState {
    pub sent: u64,
    pub acknowledged: u64,
}

/// Shared map of outgoing message states, keyed by msgno.
pub(super) type OutgoingMap = Arc<Mutex<HashMap<u64, OutgoingState>>>;

/// Read packets from the stream, assemble messages, deliver to pending map.
pub(super) async fn reader_task(
    reader: impl AsyncRead + Unpin,
    pending: Arc<Mutex<HashMap<i64, oneshot::Sender<ScampResponse>>>>,
    writer_tx: mpsc::Sender<Packet>,
    outgoing: OutgoingMap,
    ack_notify: Arc<Notify>,
) {
    let mut reader = reader;
    let mut buf = Vec::with_capacity(8192);
    let mut incoming: HashMap<u64, IncomingMessage> = HashMap::new();
    let mut next_incoming_msg_no: u64 = 0; // Starts at 0 — all implementations agree

    loop {
        let mut tmp = [0u8; 4096];
        let n = match reader.read(&mut tmp).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                log::error!("Read error: {}", e);
                break;
            }
        };
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() > MAX_PACKET_SIZE + 100 {
            log::error!("Read buffer exceeded max packet size, closing");
            break;
        }

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
                        &mut next_incoming_msg_no,
                        &pending,
                        &writer_tx,
                        &outgoing,
                        &ack_notify,
                    )
                    .await;
                }
                ParseResult::Fatal(err) => {
                    log::error!("Fatal protocol error: {}", err);
                    notify_all_pending(&pending, &format!("Protocol error: {err}")).await;
                    return;
                }
            }
        }
        buf.drain(..consumed);
    }

    // Connection closed
    notify_all_pending(&pending, "Connection lost").await;
}

async fn notify_all_pending(pending: &Arc<Mutex<HashMap<i64, oneshot::Sender<ScampResponse>>>>, error: &str) {
    let mut pend = pending.lock().await;
    for (_, tx) in pend.drain() {
        let _ = tx.send(ScampResponse {
            header: PacketHeader::default(),
            body: vec![],
            error: Some(error.to_string()),
        });
    }
}

/// Route a single packet: assemble HEADER → DATA* → EOF/TXERR.
async fn route_packet(
    packet: Packet,
    incoming: &mut HashMap<u64, IncomingMessage>,
    next_incoming_msg_no: &mut u64,
    pending: &Arc<Mutex<HashMap<i64, oneshot::Sender<ScampResponse>>>>,
    writer_tx: &mpsc::Sender<Packet>,
    outgoing: &OutgoingMap,
    ack_notify: &Arc<Notify>,
) {
    match packet.packet_type {
        PacketType::Header => {
            // Perl Connection.pm:140 — validate sequential msgno
            if packet.msg_no != *next_incoming_msg_no {
                log::error!("Out of sequence: expected {} got {}", *next_incoming_msg_no, packet.msg_no);
                return;
            }
            *next_incoming_msg_no += 1;
            if let Some(header) = packet.packet_header {
                incoming.insert(
                    packet.msg_no,
                    IncomingMessage {
                        header,
                        body: Vec::new(),
                        received: 0,
                    },
                );
            }
        }
        PacketType::Data => {
            let Some(msg) = incoming.get_mut(&packet.msg_no) else {
                log::error!("DATA with no active message for msgno {}", packet.msg_no);
                return;
            };
            if packet.body.is_empty() {
                return;
            } // JS connection.js:202
            msg.body.extend_from_slice(&packet.body);
            msg.received += packet.body.len();

            // ACK: cumulative bytes as decimal string (Perl Connection.pm:153)
            let ack = Packet {
                packet_type: PacketType::Ack,
                msg_no: packet.msg_no,
                packet_header: None,
                body: msg.received.to_string().into_bytes(),
            };
            let _ = writer_tx.send(ack).await;
        }
        PacketType::Eof => {
            // Perl Connection.pm:162 — EOF body must be empty
            if !packet.body.is_empty() {
                log::error!("EOF packet must be empty");
                return;
            }
            let Some(msg) = incoming.remove(&packet.msg_no) else {
                log::error!("EOF with no active message for msgno {}", packet.msg_no);
                return;
            };
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
            // JS connection.js:229 — empty/"0" TXERR body is invalid
            let body_str = String::from_utf8_lossy(&packet.body);
            if body_str.is_empty() || body_str == "0" {
                log::error!("TXERR with empty/zero body for msgno {}", packet.msg_no);
                return;
            }
            let Some(msg) = incoming.remove(&packet.msg_no) else {
                log::error!("TXERR with no active message for msgno {}", packet.msg_no);
                return;
            };
            let error_text = body_str.to_string();
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
            // Perl Connection.pm:177-183 — validate ACK value
            let body_str = String::from_utf8_lossy(&packet.body);
            let ack_val: u64 = match body_str.parse() {
                Ok(v) if v > 0 => v,
                _ => {
                    log::error!("Malformed ACK body: {:?}", body_str);
                    return;
                }
            };
            let mut out = outgoing.lock().await;
            if let Some(state) = out.get_mut(&packet.msg_no) {
                if ack_val <= state.acknowledged {
                    log::error!("ACK pointer moved backward: {} <= {}", ack_val, state.acknowledged);
                    return;
                }
                if ack_val > state.sent {
                    log::error!("ACK pointer past end: {} > sent {}", ack_val, state.sent);
                    return;
                }
                state.acknowledged = ack_val;
                // D5b: Wake sender if it's blocked on flow control watermark
                ack_notify.notify_one();
            }
        }
        PacketType::Ping => {
            let pong = Packet {
                packet_type: PacketType::Pong,
                msg_no: packet.msg_no,
                packet_header: None,
                body: vec![],
            };
            let _ = writer_tx.send(pong).await;
        }
        PacketType::Pong => {
            // Heartbeat response — would reset timer (not implemented)
        }
    }
}

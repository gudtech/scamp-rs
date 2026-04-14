//! Reply writing for server connections.
//! Extracted from server_connection.rs to stay under 300-line limit.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::AsyncWriteExt;

use super::handler::ScampReply;
use super::server_connection::{OutgoingReplyState, ServerWriter};
use crate::transport::beepish::proto::{EnvelopeFormat, FlexInt, MessageType, Packet, PacketHeader, PacketType, DATA_CHUNK_SIZE};

pub(crate) async fn send_reply(
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

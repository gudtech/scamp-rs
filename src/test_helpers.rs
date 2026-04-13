//! Shared test helpers for scamp tests (T8).
//! Provides common fixtures and utilities used across test modules.

#![cfg(test)]

use std::collections::HashMap;
use std::sync::Arc;

use crate::service::handler::{RegisteredAction, ScampReply};
use crate::transport::beepish::proto::{FlexInt, MessageType, Packet, PacketHeader, PacketType, ParseResult, DATA_CHUNK_SIZE};
use tokio::io::AsyncWriteExt;

/// Create a registered action map with an echo handler for testing.
pub fn echo_actions() -> Arc<HashMap<String, RegisteredAction>> {
    let mut actions = HashMap::new();
    actions.insert(
        "echo.v1".to_string(),
        RegisteredAction {
            name: "echo".to_string(),
            version: 1,
            flags: vec![],
            handler: Arc::new(|req| Box::pin(async move { ScampReply::ok(req.body) })),
        },
    );
    Arc::new(actions)
}

/// Build a request PacketHeader with common defaults.
pub fn make_request_header(action: &str, version: i32, request_id: i64) -> PacketHeader {
    PacketHeader {
        action: action.to_string(),
        version,
        request_id: FlexInt(request_id),
        message_type: MessageType::Request,
        ..PacketHeader::default()
    }
}

/// Write a complete SCAMP request (HEADER + DATA chunks + EOF) to a stream.
pub async fn write_request(
    writer: &mut (impl AsyncWriteExt + Unpin),
    msg_no: u64,
    action: &str,
    version: i32,
    request_id: i64,
    body: &[u8],
) {
    Packet {
        packet_type: PacketType::Header,
        msg_no,
        packet_header: Some(make_request_header(action, version, request_id)),
        body: vec![],
    }
    .write(writer)
    .await
    .unwrap();

    for chunk in body.chunks(DATA_CHUNK_SIZE) {
        Packet {
            packet_type: PacketType::Data,
            msg_no,
            packet_header: None,
            body: chunk.to_vec(),
        }
        .write(writer)
        .await
        .unwrap();
    }

    Packet {
        packet_type: PacketType::Eof,
        msg_no,
        packet_header: None,
        body: vec![],
    }
    .write(writer)
    .await
    .unwrap();

    writer.flush().await.unwrap();
}

/// Parse all complete SCAMP packets from raw bytes.
pub fn parse_all_packets(data: &[u8]) -> Vec<Packet> {
    let mut packets = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        match Packet::parse(&data[offset..]) {
            ParseResult::Success { packet, bytes_used } => {
                offset += bytes_used;
                packets.push(packet);
            }
            _ => break,
        }
    }
    packets
}

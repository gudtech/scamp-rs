use super::server_connection::handle_connection;
use crate::service::handler::RegisteredAction;
use crate::test_helpers::{echo_actions, parse_all_packets, write_request};
use crate::transport::beepish::proto::{MessageType, Packet, PacketType};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Send a single request and collect all response packets.
async fn roundtrip(actions: Arc<HashMap<String, RegisteredAction>>, action: &str, version: i32, body: &[u8]) -> Vec<Packet> {
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

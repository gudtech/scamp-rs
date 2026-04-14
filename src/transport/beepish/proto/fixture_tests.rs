// Perl wire fixture tests — parse real packets captured from the canonical Perl implementation.
// Any failure here is a wire compatibility regression.

use super::fixtures;
use super::*;

#[test]
fn test_perl_header_request() {
    let buf = fixtures::perl_request_header();
    match Packet::parse(&buf) {
        ParseResult::Success { packet, bytes_used } => {
            assert_eq!(bytes_used, buf.len());
            assert_eq!(packet.packet_type, PacketType::Header);
            assert_eq!(packet.msg_no, 0);
            let hdr = packet.packet_header.unwrap();
            assert_eq!(hdr.action, "api.status.health_check");
            assert_eq!(hdr.message_type, MessageType::Request);
            assert_eq!(hdr.request_id.0, 1);
            assert_eq!(hdr.client_id.0, 0);
            assert_eq!(hdr.envelope, EnvelopeFormat::Json);
            assert_eq!(hdr.version, 1);
        }
        _ => panic!("Failed to parse Perl HEADER request fixture"),
    }
}

#[test]
fn test_perl_header_with_nulls() {
    let buf = fixtures::perl_request_header_with_nulls();
    match Packet::parse(&buf) {
        ParseResult::Success { packet, .. } => {
            let hdr = packet.packet_header.unwrap();
            assert_eq!(hdr.action, "api.status.health_check");
            assert_eq!(hdr.ticket, ""); // null → empty string
            assert_eq!(hdr.identifying_token, ""); // null → empty string
        }
        _ => panic!("Failed to parse Perl HEADER with nulls fixture"),
    }
}

#[test]
fn test_perl_data_packet() {
    let buf = fixtures::perl_data_empty_json();
    match Packet::parse(&buf) {
        ParseResult::Success { packet, .. } => {
            assert_eq!(packet.packet_type, PacketType::Data);
            assert_eq!(packet.body, b"{}");
        }
        _ => panic!("Failed to parse Perl DATA fixture"),
    }
}

#[test]
fn test_perl_eof_packet() {
    let buf = fixtures::perl_eof();
    match Packet::parse(&buf) {
        ParseResult::Success { packet, .. } => {
            assert_eq!(packet.packet_type, PacketType::Eof);
            assert!(packet.body.is_empty());
        }
        _ => panic!("Failed to parse Perl EOF fixture"),
    }
}

#[test]
fn test_perl_ack_packet() {
    let buf = fixtures::perl_ack(2);
    match Packet::parse(&buf) {
        ParseResult::Success { packet, .. } => {
            assert_eq!(packet.packet_type, PacketType::Ack);
            assert_eq!(std::str::from_utf8(&packet.body).unwrap(), "2");
        }
        _ => panic!("Failed to parse Perl ACK fixture"),
    }
}

#[test]
fn test_perl_reply_ok() {
    let buf = fixtures::perl_reply_ok();
    match Packet::parse(&buf) {
        ParseResult::Success { packet, .. } => {
            assert_eq!(packet.packet_type, PacketType::Header);
            assert_eq!(packet.msg_no, 1);
            let hdr = packet.packet_header.unwrap();
            assert_eq!(hdr.message_type, MessageType::Reply);
            assert_eq!(hdr.request_id.0, 1);
            assert!(hdr.error.is_none());
            assert!(hdr.error_code.is_none());
        }
        _ => panic!("Failed to parse Perl reply OK fixture"),
    }
}

#[test]
fn test_perl_reply_error() {
    let buf = fixtures::perl_reply_error();
    match Packet::parse(&buf) {
        ParseResult::Success { packet, .. } => {
            let hdr = packet.packet_header.unwrap();
            assert_eq!(hdr.message_type, MessageType::Reply);
            assert_eq!(hdr.error.as_deref(), Some("Action not found"));
            assert_eq!(hdr.error_code.as_deref(), Some("not_found"));
        }
        _ => panic!("Failed to parse Perl reply error fixture"),
    }
}

#[test]
fn test_perl_txerr() {
    let buf = fixtures::perl_txerr();
    match Packet::parse(&buf) {
        ParseResult::Success { packet, .. } => {
            assert_eq!(packet.packet_type, PacketType::Txerr);
            assert_eq!(
                std::str::from_utf8(&packet.body).unwrap(),
                "Connection closed before message finished"
            );
        }
        _ => panic!("Failed to parse Perl TXERR fixture"),
    }
}

/// Parse a complete request sequence (HEADER + DATA + EOF) from a single buffer.
#[test]
fn test_perl_request_sequence() {
    let buf = fixtures::perl_request_sequence();
    let mut offset = 0;
    let mut packets = Vec::new();

    while offset < buf.len() {
        match Packet::parse(&buf[offset..]) {
            ParseResult::Success { packet, bytes_used } => {
                offset += bytes_used;
                packets.push(packet);
            }
            _ => panic!("Failed to parse at offset {}", offset),
        }
    }

    assert_eq!(packets.len(), 3);
    assert_eq!(packets[0].packet_type, PacketType::Header);
    assert_eq!(packets[1].packet_type, PacketType::Data);
    assert_eq!(packets[2].packet_type, PacketType::Eof);
}

/// Parse a complete reply sequence (HEADER + DATA + EOF) for msgno 1.
#[test]
fn test_perl_reply_sequence() {
    let buf = fixtures::perl_reply_sequence(b"{\"status\":\"ok\"}");
    let mut offset = 0;
    let mut packets = Vec::new();

    while offset < buf.len() {
        match Packet::parse(&buf[offset..]) {
            ParseResult::Success { packet, bytes_used } => {
                offset += bytes_used;
                packets.push(packet);
            }
            _ => panic!("Failed to parse at offset {}", offset),
        }
    }

    assert_eq!(packets.len(), 3);
    assert_eq!(packets[0].packet_type, PacketType::Header);
    assert_eq!(packets[1].packet_type, PacketType::Data);
    assert_eq!(packets[2].packet_type, PacketType::Eof);
    assert!(packets.iter().all(|p| p.msg_no == 1));
    assert_eq!(packets[1].body, b"{\"status\":\"ok\"}");
}

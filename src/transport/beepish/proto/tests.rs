use super::*;

#[test]
fn test_envelope_format_serde() {
    assert_eq!(serde_json::to_string(&EnvelopeFormat::Json).unwrap(), r#""json""#);
    assert_eq!(serde_json::to_string(&EnvelopeFormat::JsonStore).unwrap(), r#""jsonstore""#);
    assert_eq!(serde_json::to_string(&EnvelopeFormat::Other("extdirect".into())).unwrap(), r#""extdirect""#);

    assert_eq!(serde_json::from_str::<EnvelopeFormat>(r#""json""#).unwrap(), EnvelopeFormat::Json);
    assert_eq!(serde_json::from_str::<EnvelopeFormat>(r#""jsonstore""#).unwrap(), EnvelopeFormat::JsonStore);
    assert_eq!(serde_json::from_str::<EnvelopeFormat>(r#""web""#).unwrap(), EnvelopeFormat::Other("web".into()));
}

#[test]
fn test_message_type_serde() {
    assert_eq!(serde_json::to_string(&MessageType::Request).unwrap(), r#""request""#);
    assert_eq!(serde_json::to_string(&MessageType::Reply).unwrap(), r#""reply""#);
    assert_eq!(serde_json::from_str::<MessageType>(r#""request""#).unwrap(), MessageType::Request);
    assert_eq!(serde_json::from_str::<MessageType>(r#""reply""#).unwrap(), MessageType::Reply);
}

#[test]
fn test_flex_int_from_integer() {
    let fi: FlexInt = serde_json::from_str("42").unwrap();
    assert_eq!(fi.0, 42);
}

#[test]
fn test_flex_int_from_string() {
    let fi: FlexInt = serde_json::from_str(r#""42""#).unwrap();
    assert_eq!(fi.0, 42);
}

#[test]
fn test_flex_int_from_negative() {
    let fi: FlexInt = serde_json::from_str("-1").unwrap();
    assert_eq!(fi.0, -1);
}

#[test]
fn test_flex_int_serialize() {
    assert_eq!(serde_json::to_string(&FlexInt(42)).unwrap(), "42");
}

#[test]
fn test_packet_header_json_field_names() {
    // Wire compat: JSON field name MUST be "type", not "message_type"
    let hdr = PacketHeader {
        action: "API.Status.health_check".into(),
        envelope: EnvelopeFormat::Json,
        error: None,
        error_code: None,
        request_id: FlexInt(1),
        client_id: FlexInt(123),
        ticket: "some-ticket".into(),
        identifying_token: "".into(),
        message_type: MessageType::Request,
        version: 1,
    };

    let json = serde_json::to_string(&hdr).unwrap();
    assert!(json.contains(r#""type":"request""#), "must use 'type' not 'message_type': {json}");
    assert!(json.contains(r#""envelope":"json""#), "envelope must be lowercase: {json}");
    assert!(!json.contains("error"), "error should be omitted when None: {json}");
    assert!(!json.contains("error_code"), "error_code should be omitted when None: {json}");
    assert!(!json.contains("identifying_token"), "identifying_token should be omitted when empty: {json}");
}

#[test]
fn test_packet_header_roundtrip() {
    let hdr = PacketHeader {
        action: "Product.Sku.fetch".into(),
        envelope: EnvelopeFormat::Json,
        error: None,
        error_code: None,
        request_id: FlexInt(42),
        client_id: FlexInt(7),
        ticket: "1,100,200,1700000000,3600,admin,sig".into(),
        identifying_token: "tok".into(),
        message_type: MessageType::Request,
        version: 1,
    };

    let json = serde_json::to_string(&hdr).unwrap();
    let hdr2: PacketHeader = serde_json::from_str(&json).unwrap();
    assert_eq!(hdr2.action, hdr.action);
    assert_eq!(hdr2.request_id.0, hdr.request_id.0);
    assert_eq!(hdr2.message_type, hdr.message_type);
    assert_eq!(hdr2.envelope, hdr.envelope);
}

#[test]
fn test_packet_header_deserialize_go_format() {
    // Go packetheader.go json tags
    let go_json = r#"{"action":"Product.Sku.fetch","envelope":"json","request_id":1,"client_id":42,"ticket":"","identifying_token":"","type":"request","version":1}"#;
    let hdr: PacketHeader = serde_json::from_str(go_json).unwrap();
    assert_eq!(hdr.action, "Product.Sku.fetch");
    assert_eq!(hdr.message_type, MessageType::Request);
    assert_eq!(hdr.request_id.0, 1);
    assert_eq!(hdr.client_id.0, 42);
}

#[test]
fn test_packet_header_deserialize_flex_client_id() {
    let json = r#"{"action":"Test.action","envelope":"json","request_id":1,"client_id":"999","ticket":"","identifying_token":"","type":"request","version":1}"#;
    let hdr: PacketHeader = serde_json::from_str(json).unwrap();
    assert_eq!(hdr.client_id.0, 999);
}

#[test]
fn test_packet_parse_roundtrip() {
    let pkt = Packet {
        packet_type: PacketType::Data,
        msg_no: 3,
        packet_header: None,
        body: b"hello world".to_vec(),
    };

    let mut buf = Vec::new();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async { pkt.write(&mut buf).await.unwrap() });

    match Packet::parse(&buf) {
        ParseResult::Success { packet, bytes_used } => {
            assert_eq!(bytes_used, buf.len());
            assert_eq!(packet.packet_type, PacketType::Data);
            assert_eq!(packet.msg_no, 3);
            assert_eq!(packet.body, b"hello world");
        }
        _ => panic!("Expected Success"),
    }
}

#[test]
fn test_packet_parse_eof_empty_body() {
    // Perl Connection.pm:162 — EOF body must be empty
    let buf = b"EOF 0 0\r\nEND\r\n";
    match Packet::parse(buf) {
        ParseResult::Success { packet, .. } => {
            assert_eq!(packet.packet_type, PacketType::Eof);
            assert!(packet.body.is_empty());
        }
        _ => panic!("Expected Success"),
    }
}

#[test]
fn test_packet_parse_ack_decimal_string() {
    // Perl Connection.pm:179 — ACK body is decimal string
    let buf = b"ACK 0 6\r\n131072END\r\n";
    match Packet::parse(buf) {
        ParseResult::Success { packet, .. } => {
            assert_eq!(packet.packet_type, PacketType::Ack);
            assert_eq!(std::str::from_utf8(&packet.body).unwrap(), "131072");
        }
        _ => panic!("Expected Success"),
    }
}

#[test]
fn test_packet_parse_too_short() {
    assert!(matches!(Packet::parse(b"HEA"), ParseResult::TooShort));
    assert!(matches!(Packet::parse(b""), ParseResult::TooShort));
}

#[test]
fn test_packet_parse_need_bytes() {
    let buf = b"DATA 0 100\r\npartial";
    assert!(matches!(Packet::parse(buf), ParseResult::NeedBytes { .. }));
}

/// Perl simple_async_request sends ticket:null — must not crash deserialization.
#[test]
fn test_header_with_null_ticket() {
    let json = r#"{"action":"ScampRsTest.echo","version":1,"envelope":"json","ticket":null,"type":"request","request_id":1}"#;
    let result: Result<PacketHeader, _> = serde_json::from_str(json);
    assert!(result.is_ok(), "ticket:null should deserialize, got: {:?}", result.err());
}

/// Some implementations may send identifying_token:null too.
#[test]
fn test_header_with_null_identifying_token() {
    let json = r#"{"action":"test","version":1,"envelope":"json","identifying_token":null,"type":"request","request_id":1}"#;
    let result: Result<PacketHeader, _> = serde_json::from_str(json);
    assert!(result.is_ok(), "identifying_token:null should deserialize, got: {:?}", result.err());
}

/// Perl Connection.pm:46 requires \r\n — bare \n must be rejected.
#[test]
fn test_bare_newline_rejected() {
    let buf = b"HEADER 0 2\n{}END\r\n";
    assert!(matches!(Packet::parse(buf), ParseResult::Fatal(_)));
}

/// Valid \r\n header line must still parse correctly.
#[test]
fn test_crlf_required() {
    let json = r#"{"type":"request","action":"test","version":1,"envelope":"json","request_id":1}"#;
    let buf = format!("HEADER 0 {}\r\n{}END\r\n", json.len(), json);
    match Packet::parse(buf.as_bytes()) {
        ParseResult::Success { packet, .. } => {
            assert_eq!(packet.packet_type, PacketType::Header);
            assert!(packet.packet_header.is_some());
        }
        _ => panic!("Expected Success"),
    }
}

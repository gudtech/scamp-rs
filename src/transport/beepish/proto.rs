use anyhow::anyhow;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use tokio::io::AsyncWriteExt;

pub const MAX_PACKET_SIZE: usize = 131072;

/// Maximum DATA chunk size when sending. All receivers handle up to 131072.
/// Perl uses 2048, Go uses 128KB, JS uses 131072.
pub const DATA_CHUNK_SIZE: usize = 131072;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PacketType {
    Header,
    Data,
    Eof,
    Txerr,
    Ack,
    Ping,
    Pong,
}

/// SCAMP packet header — serialized as JSON in the body of a HEADER packet.
///
/// Field names and value formats are critical for wire compatibility:
/// - `type` (not `message_type`) — "request" or "reply"
/// - `envelope` — "json", "jsonstore", etc (lowercase strings)
/// - `request_id` — sequential integer
/// - `client_id` — FlexInt (accepts both integer and string JSON)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PacketHeader {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub action: String,

    #[serde(default)]
    pub envelope: EnvelopeFormat,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,

    #[serde(default)]
    pub request_id: FlexInt,

    #[serde(default)]
    pub client_id: FlexInt,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ticket: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub identifying_token: String,

    /// Wire format: "request" or "reply". JSON field name is "type".
    #[serde(rename = "type", default)]
    pub message_type: MessageType,

    #[serde(default)]
    pub version: i32,
}

// ---- EnvelopeFormat: custom serde as lowercase strings ----

#[derive(Debug, Clone, PartialEq)]
pub enum EnvelopeFormat {
    Json,
    JsonStore,
    Other(String),
}

impl Default for EnvelopeFormat {
    fn default() -> Self {
        EnvelopeFormat::Json
    }
}

impl Serialize for EnvelopeFormat {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            EnvelopeFormat::Json => serializer.serialize_str("json"),
            EnvelopeFormat::JsonStore => serializer.serialize_str("jsonstore"),
            EnvelopeFormat::Other(s) => serializer.serialize_str(s),
        }
    }
}

impl<'de> Deserialize<'de> for EnvelopeFormat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "json" => EnvelopeFormat::Json,
            "jsonstore" => EnvelopeFormat::JsonStore,
            other => EnvelopeFormat::Other(other.to_string()),
        })
    }
}

// ---- MessageType: custom serde as lowercase strings ----

#[derive(Debug, Clone, PartialEq)]
pub enum MessageType {
    Request,
    Reply,
}

impl Default for MessageType {
    fn default() -> Self {
        MessageType::Request
    }
}

impl Serialize for MessageType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            MessageType::Request => serializer.serialize_str("request"),
            MessageType::Reply => serializer.serialize_str("reply"),
        }
    }
}

impl<'de> Deserialize<'de> for MessageType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "request" => Ok(MessageType::Request),
            "reply" => Ok(MessageType::Reply),
            other => Err(de::Error::custom(format!("unknown message type: {other}"))),
        }
    }
}

// ---- FlexInt: deserializes from both JSON integer and JSON string ----

/// An integer type that deserializes from both JSON integer (42) and
/// JSON string ("42"). This matches Go's `flexInt` type in packetheader.go.
/// Needed because some services send client_id as a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlexInt(pub i64);

impl From<i64> for FlexInt {
    fn from(v: i64) -> Self {
        FlexInt(v)
    }
}

impl From<FlexInt> for i64 {
    fn from(v: FlexInt) -> Self {
        v.0
    }
}

impl Serialize for FlexInt {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.0)
    }
}

impl<'de> Deserialize<'de> for FlexInt {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FlexIntVisitor;

        impl<'de> Visitor<'de> for FlexIntVisitor {
            type Value = FlexInt;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "an integer or a string containing an integer")
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<FlexInt, E> {
                Ok(FlexInt(v))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<FlexInt, E> {
                Ok(FlexInt(v as i64))
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<FlexInt, E> {
                v.parse::<i64>()
                    .map(FlexInt)
                    .map_err(|_| de::Error::custom(format!("cannot parse '{v}' as integer")))
            }
        }

        deserializer.deserialize_any(FlexIntVisitor)
    }
}

impl fmt::Display for FlexInt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---- Packet ----

pub struct Packet {
    pub packet_type: PacketType,
    pub msg_no: u64,
    pub packet_header: Option<PacketHeader>,
    pub body: Vec<u8>,
}

pub enum ParseResult {
    TooShort,
    NeedBytes { bytes: usize },
    Success { packet: Packet, bytes_used: usize },
    Drop { bytes_used: usize },
    Fatal(anyhow::Error),
}

impl Packet {
    pub async fn write<W>(&self, writer: &mut W) -> std::io::Result<usize>
    where
        W: AsyncWriteExt + Unpin,
    {
        let packet_type_bytes: &[u8] = match self.packet_type {
            PacketType::Header => b"HEADER",
            PacketType::Data => b"DATA",
            PacketType::Eof => b"EOF",
            PacketType::Txerr => b"TXERR",
            PacketType::Ack => b"ACK",
            PacketType::Ping => b"PING",
            PacketType::Pong => b"PONG",
        };

        let mut body_buf = Vec::new();
        if let Some(header) = &self.packet_header {
            serde_json::to_writer(&mut body_buf, header)?;
        } else {
            body_buf.extend_from_slice(&self.body);
        }

        let header_bytes = format!(
            "{} {} {}\r\n",
            std::str::from_utf8(packet_type_bytes).unwrap(),
            self.msg_no,
            body_buf.len()
        );

        writer.write_all(header_bytes.as_bytes()).await?;
        writer.write_all(&body_buf).await?;
        writer.write_all(b"END\r\n").await?;

        Ok(header_bytes.len() + body_buf.len() + 5)
    }

    pub fn parse(buf: &[u8]) -> ParseResult {
        // Need at least a few bytes to look at the header line
        if buf.is_empty() {
            return ParseResult::TooShort;
        }

        let hdr_len = buf.len().min(80); // Perl limits header line to 80 bytes
        let hdr = match std::str::from_utf8(&buf[..hdr_len]) {
            Ok(hdr) => hdr.trim_end_matches('\0'),
            Err(_) => return ParseResult::Fatal(anyhow!("Invalid UTF-8 in header")),
        };

        let Some(cut) = hdr.find('\n') else {
            if hdr_len >= 80 {
                return ParseResult::Fatal(anyhow!("Overlong header line"));
            }
            return ParseResult::TooShort;
        };

        let header_line = &hdr[..cut + 1];
        let parts: Vec<&str> = header_line.split_whitespace().collect();
        if parts.len() != 3 {
            return ParseResult::Fatal(anyhow!("Malformed header line"));
        }

        let cmd = parts[0];
        let msg_no: u64 = match parts[1].parse() {
            Ok(n) => n,
            Err(_) => return ParseResult::Fatal(anyhow!("Invalid message number")),
        };

        let siz: usize = match parts[2].parse() {
            Ok(n) => n,
            Err(_) => return ParseResult::Fatal(anyhow!("Invalid packet size")),
        };

        if siz > MAX_PACKET_SIZE {
            return ParseResult::Fatal(anyhow!("Unreasonably large packet"));
        }

        let payload_start = header_line.len();
        let payload_end = payload_start + siz;

        if payload_end + 5 > buf.len() {
            return ParseResult::NeedBytes {
                bytes: payload_end + 5 - buf.len(),
            };
        }

        let payload = &buf[payload_start..payload_end];
        let trailer = &buf[payload_end..payload_end + 5];

        if trailer != b"END\r\n" {
            return ParseResult::Fatal(anyhow!("Malformed trailer"));
        }

        let packet_type = match cmd {
            "HEADER" => PacketType::Header,
            "DATA" => PacketType::Data,
            "EOF" => PacketType::Eof,
            "TXERR" => PacketType::Txerr,
            "ACK" => PacketType::Ack,
            "PING" => PacketType::Ping,
            "PONG" => PacketType::Pong,
            // Drop otherwise-valid but unknown packet types (forward compat)
            _ => {
                return ParseResult::Drop {
                    bytes_used: payload_end + 5,
                }
            }
        };

        let packet = Packet {
            packet_header: if packet_type == PacketType::Header {
                match serde_json::from_slice(payload) {
                    Ok(header) => Some(header),
                    Err(_) => {
                        return ParseResult::Drop {
                            bytes_used: payload_end + 5,
                        };
                    }
                }
            } else {
                None
            },
            body: if packet_type != PacketType::Header {
                payload.to_vec()
            } else {
                vec![]
            },
            packet_type,
            msg_no,
        };

        ParseResult::Success {
            packet,
            bytes_used: payload_end + 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_format_serde() {
        // Serialize
        assert_eq!(
            serde_json::to_string(&EnvelopeFormat::Json).unwrap(),
            r#""json""#
        );
        assert_eq!(
            serde_json::to_string(&EnvelopeFormat::JsonStore).unwrap(),
            r#""jsonstore""#
        );
        assert_eq!(
            serde_json::to_string(&EnvelopeFormat::Other("extdirect".into())).unwrap(),
            r#""extdirect""#
        );

        // Deserialize
        assert_eq!(
            serde_json::from_str::<EnvelopeFormat>(r#""json""#).unwrap(),
            EnvelopeFormat::Json
        );
        assert_eq!(
            serde_json::from_str::<EnvelopeFormat>(r#""jsonstore""#).unwrap(),
            EnvelopeFormat::JsonStore
        );
        assert_eq!(
            serde_json::from_str::<EnvelopeFormat>(r#""web""#).unwrap(),
            EnvelopeFormat::Other("web".into())
        );
    }

    #[test]
    fn test_message_type_serde() {
        assert_eq!(
            serde_json::to_string(&MessageType::Request).unwrap(),
            r#""request""#
        );
        assert_eq!(
            serde_json::to_string(&MessageType::Reply).unwrap(),
            r#""reply""#
        );
        assert_eq!(
            serde_json::from_str::<MessageType>(r#""request""#).unwrap(),
            MessageType::Request
        );
        assert_eq!(
            serde_json::from_str::<MessageType>(r#""reply""#).unwrap(),
            MessageType::Reply
        );
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
        let fi = FlexInt(42);
        assert_eq!(serde_json::to_string(&fi).unwrap(), "42");
    }

    #[test]
    fn test_packet_header_json_field_names() {
        // This test verifies wire compatibility with Perl/Go/JS.
        // The JSON field name MUST be "type", not "message_type".
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

        // Verify "type" field name (not "message_type")
        assert!(json.contains(r#""type":"request""#), "JSON must use 'type' not 'message_type': {json}");

        // Verify lowercase envelope
        assert!(json.contains(r#""envelope":"json""#), "envelope must be lowercase: {json}");

        // Verify optional fields are omitted when None/empty
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
        // Simulate a header that Go would produce (from packetheader.go json tags)
        let go_json = r#"{
            "action": "Product.Sku.fetch",
            "envelope": "json",
            "request_id": 1,
            "client_id": 42,
            "ticket": "",
            "identifying_token": "",
            "type": "request",
            "version": 1
        }"#;

        let hdr: PacketHeader = serde_json::from_str(go_json).unwrap();
        assert_eq!(hdr.action, "Product.Sku.fetch");
        assert_eq!(hdr.message_type, MessageType::Request);
        assert_eq!(hdr.envelope, EnvelopeFormat::Json);
        assert_eq!(hdr.request_id.0, 1);
        assert_eq!(hdr.client_id.0, 42);
    }

    #[test]
    fn test_packet_header_deserialize_flex_client_id() {
        // Some services send client_id as a string
        let json = r#"{
            "action": "Test.action",
            "envelope": "json",
            "request_id": 1,
            "client_id": "999",
            "ticket": "",
            "identifying_token": "",
            "type": "request",
            "version": 1
        }"#;

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
        // Use blocking write for test
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            pkt.write(&mut buf).await.unwrap();
        });

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
        // EOF packets must have empty body (Perl Connection.pm:162)
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
        // ACK body is a decimal string (Perl Connection.pm:179)
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
        // Header line complete but body not fully received
        let buf = b"DATA 0 100\r\npartial";
        assert!(matches!(
            Packet::parse(buf),
            ParseResult::NeedBytes { .. }
        ));
    }
}

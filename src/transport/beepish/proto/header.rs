//! PacketHeader JSON structure and custom serde for wire types.
//!
//! Field names and serialization must match Perl/Go/JS exactly.

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// SCAMP packet header — serialized as JSON in the body of a HEADER packet.
///
/// Field names and value formats are critical for wire compatibility:
/// - `type` (not `message_type`) — "request" or "reply"
/// - `envelope` — "json", "jsonstore", etc (lowercase strings)
/// - `request_id` — sequential integer
/// - `client_id` — FlexInt (accepts both integer and string JSON)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PacketHeader {
    #[serde(default, deserialize_with = "nullable_string")]
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

    #[serde(default, deserialize_with = "nullable_string")]
    pub ticket: String,

    #[serde(default, deserialize_with = "nullable_string")]
    pub identifying_token: String,

    /// Wire format: "request" or "reply". JSON field name is "type".
    #[serde(rename = "type", default)]
    pub message_type: MessageType,

    #[serde(default)]
    pub version: i32,
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

/// Deserialize a JSON value as String, treating null as empty string.
/// Perl sends `ticket => undef` which JSON-encodes as `"ticket": null`.
fn nullable_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Option::<String>::deserialize(deserializer).map(|opt| opt.unwrap_or_default())
}

// ---- EnvelopeFormat ----

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

// ---- MessageType ----

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

// ---- FlexInt ----

/// Deserializes from both JSON integer (42) and JSON string ("42").
/// Matches Go's `flexInt` type in packetheader.go.
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

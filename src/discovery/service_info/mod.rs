//! Service announcement body types and parsing.

mod parse;
#[cfg(test)]
mod tests;

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, net::SocketAddr};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ServiceInfo {
    pub identity: String,
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

impl ServiceInfo {
    /// Parse the socket address from the URI (e.g., `beepish+tls://10.0.0.1:30100`).
    pub fn socket_addr(&self) -> Result<SocketAddr, String> {
        let addr = self.uri.split("://").nth(1)
            .ok_or_else(|| format!("invalid URI (no ://): {}", self.uri))?;
        let host = addr.split(':').next()
            .ok_or_else(|| format!("invalid URI (no host): {}", self.uri))?;
        let port = addr.split(':').nth(1)
            .ok_or_else(|| format!("invalid URI (no port): {}", self.uri))?;
        let ip = host.parse().map_err(|e| format!("invalid host '{}': {}", host, e))?;
        let port = port.parse().map_err(|e| format!("invalid port '{}': {}", port, e))?;
        Ok(SocketAddr::new(ip, port))
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct AnnouncementParams {
    pub weight: u32,
    pub interval: u32,
    pub timestamp: f64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct AnnouncementBody {
    pub info: ServiceInfo,
    pub params: AnnouncementParams,
    pub actions: Vec<Action>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Action {
    pub path: String,
    pub version: u32,
    #[serde(default)]
    pub pathver: String,
    pub flags: Vec<Flag>,
    pub sector: String,
    pub envelopes: Vec<String>,
    pub packet_section: PacketSection,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum Flag {
    NoAuth,
    Timeout(u32),
    Other(String),
    CrudOp(CrudOp),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum CrudOp {
    Create,
    Read,
    Update,
    Delete,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum PacketSection {
    V3,
    V4,
}

#[derive(Debug)]
pub enum ServiceInfoParseError {
    ExpectedJsonArray,
    InvalidRootArray,
    MissingField(&'static str),
    InvalidField(&'static str),
    JsonError(serde_json::Error),
    RLEValue(&'static str, usize, serde_json::Error),
    RLEChunkLen(&'static str, usize, usize),
    RLERepeatCount(&'static str, usize),
    InvalidV3Namespace(usize),
    InvalidV3Action(usize, usize, &'static str),
}

impl std::error::Error for ServiceInfoParseError {}

impl fmt::Display for ServiceInfoParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::ExpectedJsonArray => write!(f, "Expected JSON array"),
            Self::InvalidRootArray => write!(f, "Invalid array length"),
            Self::MissingField(field) => write!(f, "Missing field: {}", field),
            Self::InvalidField(field) => write!(f, "Invalid field: {}", field),
            Self::JsonError(e) => write!(f, "JSON error: {}", e),
            Self::RLEValue(name, i, e) => write!(f, "RLE value error: {} at {}: {}", name, i, e),
            Self::RLEChunkLen(name, i, len) => write!(f, "RLE chunk len: {} at {}: {}", name, i, len),
            Self::RLERepeatCount(name, i) => write!(f, "RLE repeat count: {} at {}", name, i),
            Self::InvalidV3Namespace(i) => write!(f, "Invalid v3 namespace at {}", i),
            Self::InvalidV3Action(ns_i, ac_i, reason) => write!(f, "Invalid v3 action at ns {} ac {} {}", ns_i, ac_i, reason),
        }
    }
}

impl From<serde_json::Error> for ServiceInfoParseError {
    fn from(e: serde_json::Error) -> Self {
        ServiceInfoParseError::JsonError(e)
    }
}

impl AnnouncementBody {
    pub fn parse(v: &str) -> Result<Self, ServiceInfoParseError> {
        let value: Value = serde_json::from_str(v)?;
        let array = value.as_array().ok_or(ServiceInfoParseError::ExpectedJsonArray)?;
        if array.len() != 9 { return Err(ServiceInfoParseError::InvalidRootArray); }

        let version = array[0].as_u64().ok_or(ServiceInfoParseError::MissingField("version"))?;
        if version != 3 { return Err(ServiceInfoParseError::InvalidField("version")); }

        let identity = array[1].as_str().ok_or(ServiceInfoParseError::MissingField("identity"))?.to_string();
        let v3_sector = array[2].as_str().ok_or(ServiceInfoParseError::MissingField("sector"))?.to_string();
        let weight = array[3].as_u64().ok_or(ServiceInfoParseError::MissingField("weight"))? as u32;
        let interval = array[4].as_u64().ok_or(ServiceInfoParseError::MissingField("interval"))? as u32;
        let uri = array[5].as_str().ok_or(ServiceInfoParseError::MissingField("uri"))?.to_string();

        let envelopes_and_v4 = array[6].as_array().ok_or(ServiceInfoParseError::MissingField("envelopes_and_v4actions"))?;
        let v3_actions = array[7].as_array().ok_or(ServiceInfoParseError::MissingField("v3_actions"))?;
        let timestamp = array[8].as_f64().ok_or(ServiceInfoParseError::MissingField("timestamp"))?;

        let mut v3_envelopes: Vec<String> = Vec::new();
        let mut actions: Vec<Action> = Vec::new();

        for value in envelopes_and_v4 {
            match value {
                Value::String(envelope) => v3_envelopes.push(envelope.to_string()),
                Value::Object(obj) => parse::parse_v4_actions(obj, &mut actions)?,
                _ => {}
            }
        }
        parse::parse_v3_actions(v3_actions, &v3_sector, &v3_envelopes, &mut actions)?;

        Ok(AnnouncementBody {
            info: ServiceInfo { identity, uri, fingerprint: None },
            params: AnnouncementParams { weight, interval, timestamp },
            actions,
        })
    }
}

impl CrudOp {
    pub(crate) fn parse_str(v: &str) -> Option<Self> {
        match v {
            "create" => Some(CrudOp::Create),
            "read" => Some(CrudOp::Read),
            "update" => Some(CrudOp::Update),
            "destroy" => Some(CrudOp::Delete),
            _ => None,
        }
    }
}

impl Flag {
    pub(crate) fn parse_str(v: &str) -> Self {
        static TIMEOUT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^t(\d+)$").unwrap());
        match v {
            "noauth" => Flag::NoAuth,
            _ => {
                if let Some(caps) = TIMEOUT_RE.captures(v) {
                    Flag::Timeout(caps[1].parse().unwrap())
                } else if let Some(crud) = CrudOp::parse_str(v) {
                    Flag::CrudOp(crud)
                } else {
                    Flag::Other(v.to_string())
                }
            }
        }
    }
}

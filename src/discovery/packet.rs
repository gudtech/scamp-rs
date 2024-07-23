use std::fmt;

use super::service_info::{AnnouncementBody, ServiceInfoParseError};

#[derive(Debug)]
pub struct AnnouncementPacket {
    pub body: AnnouncementBody,
    pub certificate: String,
    pub signature: String,
}

#[derive(Debug)]
pub enum AnnouncementParseError {
    MissingJson,
    MissingCertificate,
    MissingSignature,
    TooManyParts,
    ServiceInfoParseError(ServiceInfoParseError),
    ExpectedJsonArray,
}

impl std::error::Error for AnnouncementParseError {}

impl fmt::Display for AnnouncementParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            AnnouncementParseError::MissingJson => write!(f, "Missing JSON"),
            AnnouncementParseError::MissingCertificate => write!(f, "Missing certificate"),
            AnnouncementParseError::MissingSignature => write!(f, "Missing signature"),
            AnnouncementParseError::TooManyParts => write!(f, "Too many parts in announcement"),
            AnnouncementParseError::ServiceInfoParseError(ref e) => {
                write!(f, "Failed to parse service info: {}", e)
            }
            AnnouncementParseError::ExpectedJsonArray => {
                write!(f, "Expected a JSON array")
            }
        }
    }
}

impl AnnouncementPacket {
    pub fn parse(v: &str) -> Result<Self, AnnouncementParseError> {
        let mut parts = v.split("\n\n");
        let json_blob = parts.next().ok_or(AnnouncementParseError::MissingJson)?;
        let cert_pem = parts
            .next()
            .ok_or(AnnouncementParseError::MissingCertificate)?;
        let sig_base64 = parts
            .next()
            .ok_or(AnnouncementParseError::MissingSignature)?;

        if let Some(not_empty) = parts.next() {
            if not_empty.len() > 0 {
                println!("TOOMANYPARTS {not_empty:?}");
                return Err(AnnouncementParseError::TooManyParts);
            }
        }

        let service_info = AnnouncementBody::parse(json_blob)?;

        Ok(AnnouncementPacket {
            body: service_info,
            certificate: cert_pem.to_string(),
            signature: sig_base64.to_string(),
        })
    }
    pub fn signature_is_valid(&self) -> bool {
        // TODO
        true
    }
}

impl From<ServiceInfoParseError> for AnnouncementParseError {
    fn from(err: ServiceInfoParseError) -> Self {
        AnnouncementParseError::ServiceInfoParseError(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn single_announcement() {
        let announcement = AnnouncementPacket::parse(include_str!(
            "../../samples/service_info_packet_v3_full.txt"
        ))
        .unwrap();
        println!("{:?}", announcement);
    }
}

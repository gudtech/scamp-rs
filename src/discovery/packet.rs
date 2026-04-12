use log;
use std::fmt;

use super::service_info::{AnnouncementBody, ServiceInfoParseError};

#[derive(Debug)]
pub struct AnnouncementPacket {
    /// The raw JSON blob (signed content). Preserved for signature verification.
    pub json_blob: String,
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

        let mut announcement_body = AnnouncementBody::parse(json_blob)?;

        // Compute certificate fingerprint and store in ServiceInfo
        if let Ok(fp) = crate::crypto::cert_pem_fingerprint(cert_pem) {
            announcement_body.info.fingerprint = Some(fp);
        }

        Ok(AnnouncementPacket {
            json_blob: json_blob.to_string(),
            body: announcement_body,
            certificate: cert_pem.to_string(),
            signature: sig_base64.to_string(),
        })
    }

    /// Verify the RSA PKCS1v15 SHA256 signature of this announcement.
    /// The signed content is the JSON blob (position 0 of the `\n\n`-split packet).
    /// Matches Perl ServiceInfo.pm:91-108 and Go verify.go:20-37.
    pub fn signature_is_valid(&self) -> bool {
        match crate::crypto::verify_rsa_sha256(
            &self.certificate,
            self.json_blob.as_bytes(),
            &self.signature,
        ) {
            Ok(valid) => valid,
            Err(e) => {
                log::error!("Signature verification failed: {}", e);
                false
            }
        }
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

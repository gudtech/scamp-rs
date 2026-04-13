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
        let cert_pem = parts.next().ok_or(AnnouncementParseError::MissingCertificate)?;
        let sig_base64 = parts.next().ok_or(AnnouncementParseError::MissingSignature)?;

        if let Some(not_empty) = parts.next() {
            if !not_empty.is_empty() {
                log::warn!("Announcement has extra parts after signature: {:?}", not_empty);
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
        match crate::crypto::verify_rsa_sha256(&self.certificate, self.json_blob.as_bytes(), &self.signature) {
            Ok(valid) => {
                if !valid {
                    log::warn!("Signature verification returned false for {}", self.body.info.identity);
                }
                valid
            }
            Err(e) => {
                log::error!("Signature verification error for {}: {}", self.body.info.identity, e);
                eprintln!("Signature verification error for {}: {}", self.body.info.identity, e);
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
    fn single_announcement_parses() {
        let announcement = AnnouncementPacket::parse(include_str!("../../samples/service_info_packet_v3_full.txt")).unwrap();
        assert!(!announcement.body.info.identity.is_empty());
        assert!(!announcement.certificate.is_empty());
        assert!(!announcement.signature.is_empty());
    }

    #[test]
    fn single_announcement_has_fingerprint() {
        let announcement = AnnouncementPacket::parse(include_str!("../../samples/service_info_packet_v3_full.txt")).unwrap();
        let fp = announcement.body.info.fingerprint.as_ref().unwrap();
        // Fingerprint should be colon-separated uppercase hex
        assert!(fp.contains(':'));
        assert!(fp.chars().filter(|c| c.is_ascii_alphabetic()).all(|c| c.is_ascii_uppercase()));
    }

    /// Verify ALL announcements in the live discovery cache have valid signatures.
    /// This is the M2 verification test.
    /// Run with: cargo test -- --ignored test_verify_real_cache_signatures
    #[test]
    #[ignore] // requires live dev environment
    fn test_verify_live_signature_and_tamper() {
        use crate::discovery::cache_file::CacheFileAnnouncementIterator;

        let home = std::env::var("HOME").unwrap_or_default();
        let cache_path = format!("{}/GT/backplane/discovery/discovery", home);
        let file = std::fs::File::open(&cache_path).expect("Discovery cache not found");

        // Find the first valid announcement
        let mut valid_raw = None;
        for result in CacheFileAnnouncementIterator::new(file) {
            if let Ok(ann) = result {
                if ann.signature_is_valid() {
                    valid_raw = Some(format!("{}\n\n{}\n\n{}", ann.json_blob, ann.certificate, ann.signature));
                    break;
                }
            }
        }

        let raw = valid_raw.expect("No valid announcement found in cache");

        // Tamper with it — signature should now fail
        let tampered = raw.replacen("main", "TAMPERED", 1);
        let tampered_ann = AnnouncementPacket::parse(&tampered).unwrap();
        assert!(
            !tampered_ann.signature_is_valid(),
            "Tampered announcement should fail signature verification"
        );
    }

    /// Verify ALL announcements in the live discovery cache.
    /// Run with: cargo test -- --ignored test_verify_real_cache_signatures
    #[test]
    #[ignore] // requires live dev environment
    fn test_verify_real_cache_signatures() {
        use crate::discovery::cache_file::CacheFileAnnouncementIterator;

        let home = std::env::var("HOME").unwrap_or_default();
        let cache_path = format!("{}/GT/backplane/discovery/discovery", home);
        let file = std::fs::File::open(&cache_path).expect("Discovery cache not found — is the dev environment running?");

        let mut total = 0;
        let mut verified = 0;
        let mut parse_errors = 0;

        for result in CacheFileAnnouncementIterator::new(file) {
            total += 1;
            match result {
                Ok(announcement) => {
                    if announcement.signature_is_valid() {
                        verified += 1;
                        println!(
                            "  ✓ {} ({})",
                            announcement.body.info.identity,
                            announcement.body.info.fingerprint.as_deref().unwrap_or("no fp")
                        );
                    } else {
                        println!("  ✗ {} SIGNATURE INVALID", announcement.body.info.identity);
                    }
                }
                Err(e) => {
                    parse_errors += 1;
                    println!("  ? Parse error: {}", e);
                }
            }
        }

        println!("\n{} total, {} verified, {} parse errors", total, verified, parse_errors);
        assert!(verified > 0, "No announcements verified — is the dev environment running?");
        assert_eq!(
            verified,
            total - parse_errors,
            "Some valid announcements failed signature verification"
        );
    }
}

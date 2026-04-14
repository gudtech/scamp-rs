//! SCAMP ticket parsing and verification — ticket.js, ticket.go.
//!
//! Ticket format: CSV `version,user_id,client_id,validity_start,ttl,privs,signature`
//! Signature: RSA PKCS1v15 SHA256 over fields 0-5, Base64URL encoded.

use anyhow::{anyhow, Result};
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use openssl::sign::Verifier;

/// A parsed and verified SCAMP ticket.
#[derive(Debug, Clone)]
pub struct Ticket {
    pub version: u32,
    pub user_id: u64,
    pub client_id: u64,
    pub validity_start: u64,
    pub ttl: u64,
    pub privileges: Vec<u64>,
}

impl Ticket {
    /// Parse a ticket string without verification (for inspection).
    pub fn parse(ticket_str: &str) -> Result<(Self, Vec<u8>)> {
        let parts: Vec<&str> = ticket_str.split(',').collect();
        if parts.len() < 6 {
            return Err(anyhow!("Ticket has {} fields, expected at least 6", parts.len()));
        }

        let version: u32 = parts[0].parse().map_err(|_| anyhow!("Invalid ticket version: {}", parts[0]))?;
        if version != 1 {
            return Err(anyhow!("Unsupported ticket version: {}", version));
        }

        let user_id: u64 = parts[1].parse().map_err(|_| anyhow!("Invalid user_id: {}", parts[1]))?;
        let client_id: u64 = parts[2].parse().map_err(|_| anyhow!("Invalid client_id: {}", parts[2]))?;
        let validity_start: u64 = parts[3].parse().map_err(|_| anyhow!("Invalid validity_start: {}", parts[3]))?;
        let ttl: u64 = parts[4].parse().map_err(|_| anyhow!("Invalid ttl: {}", parts[4]))?;

        let privileges: Vec<u64> = if parts[5].is_empty() {
            vec![]
        } else {
            parts[5]
                .split('+')
                .map(|s| s.parse().map_err(|_| anyhow!("Invalid privilege: {}", s)))
                .collect::<Result<Vec<_>>>()?
        };

        // Signature is the last field (may be field 6 or appended after privs)
        let sig_str = if parts.len() > 6 { parts[6] } else { "" };
        let signature = base64url_decode(sig_str)?;

        let ticket = Ticket {
            version,
            user_id,
            client_id,
            validity_start,
            ttl,
            privileges,
        };

        Ok((ticket, signature))
    }

    /// Parse and verify a ticket against a public key.
    pub fn verify(ticket_str: &str, public_key_pem: &[u8]) -> Result<Self> {
        let (ticket, signature) = Self::parse(ticket_str)?;

        // Signed data: everything before the last comma (fields 0-5)
        let signed_data = match ticket_str.rfind(',') {
            Some(pos) => &ticket_str[..pos],
            None => return Err(anyhow!("Ticket missing signature field")),
        };

        // Verify RSA PKCS1v15 SHA256 signature
        let rsa = Rsa::public_key_from_pem(public_key_pem).map_err(|e| anyhow!("Invalid ticket verify public key: {}", e))?;
        let pkey = PKey::from_rsa(rsa)?;
        let mut verifier = Verifier::new(MessageDigest::sha256(), &pkey)?;
        verifier.update(signed_data.as_bytes())?;
        if !verifier.verify(&signature)? {
            return Err(anyhow!("Ticket signature verification failed"));
        }

        // Check expiry
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now < ticket.validity_start {
            return Err(anyhow!("Ticket not yet valid (starts at {})", ticket.validity_start));
        }
        if now >= ticket.validity_start + ticket.ttl {
            return Err(anyhow!("Ticket expired"));
        }

        Ok(ticket)
    }

    /// Check if the ticket has a specific privilege ID.
    pub fn has_privilege(&self, privilege_id: u64) -> bool {
        self.privileges.contains(&privilege_id)
    }

    /// Check if the ticket has all required privilege IDs.
    pub fn has_all_privileges(&self, required: &[u64]) -> bool {
        required.iter().all(|p| self.privileges.contains(p))
    }
}

/// Decode Base64URL (RFC 4648 §5) — ticket.js uses URL-safe base64.
fn base64url_decode(s: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    engine
        .decode(s)
        .map_err(|e| anyhow!("Invalid base64url in ticket signature: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ticket_fields() {
        let ticket_str = "1,42,100,1700000000,3600,1+2+3,AAAA";
        let (ticket, _sig) = Ticket::parse(ticket_str).unwrap();
        assert_eq!(ticket.version, 1);
        assert_eq!(ticket.user_id, 42);
        assert_eq!(ticket.client_id, 100);
        assert_eq!(ticket.validity_start, 1700000000);
        assert_eq!(ticket.ttl, 3600);
        assert_eq!(ticket.privileges, vec![1, 2, 3]);
    }

    #[test]
    fn test_parse_ticket_no_privileges() {
        let ticket_str = "1,42,100,1700000000,3600,,AAAA";
        let (ticket, _sig) = Ticket::parse(ticket_str).unwrap();
        assert!(ticket.privileges.is_empty());
    }

    #[test]
    fn test_parse_ticket_too_few_fields() {
        let result = Ticket::parse("1,42,100");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_ticket_bad_version() {
        let result = Ticket::parse("2,42,100,1700000000,3600,,AAAA");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported ticket version"));
    }

    #[test]
    fn test_has_privilege() {
        let ticket = Ticket {
            version: 1,
            user_id: 1,
            client_id: 1,
            validity_start: 0,
            ttl: 0,
            privileges: vec![10, 20, 30],
        };
        assert!(ticket.has_privilege(20));
        assert!(!ticket.has_privilege(99));
        assert!(ticket.has_all_privileges(&[10, 30]));
        assert!(!ticket.has_all_privileges(&[10, 99]));
    }
}

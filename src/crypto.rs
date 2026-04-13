//! Cryptographic utilities for SCAMP: fingerprinting, signature verification.

use anyhow::{anyhow, Result};

/// Compute SHA1 fingerprint of a DER-encoded certificate.
/// Returns uppercase hex, colon-separated: `"XX:XX:XX:..."`.
///
/// Matches Perl `ServiceInfo.pm:82-87`:
///   `uc Digest->new('SHA-1')->add(_unpem($self->cert_pem))->hexdigest`
///   then `$hash =~ s/(..)(?!$)/$1:/g`
///
/// And Go `cert.go:14-31`:
///   `sha1.New() + hex.EncodeToString + ToUpper + colon-separated`
pub fn cert_sha1_fingerprint(cert_der: &[u8]) -> String {
    use openssl::hash::{hash, MessageDigest};
    let digest = hash(MessageDigest::sha1(), cert_der).expect("SHA1 hash failed");
    let hex: Vec<String> = digest.iter().map(|b| format!("{:02X}", b)).collect();
    hex.join(":")
}

/// Decode PEM to DER bytes (strips headers, base64 decodes).
/// Handles certificates and other PEM-encoded data.
pub fn pem_to_der(pem: &str) -> Result<Vec<u8>> {
    let lines: Vec<&str> = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    let b64 = lines.join("");
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &b64)
        .map_err(|e| anyhow!("base64 decode failed: {}", e))
}

/// Compute SHA1 fingerprint from a PEM-encoded certificate string.
pub fn cert_pem_fingerprint(cert_pem: &str) -> Result<String> {
    let der = pem_to_der(cert_pem)?;
    Ok(cert_sha1_fingerprint(&der))
}

/// Verify an RSA PKCS1v15 SHA256 signature.
///
/// All SCAMP implementations use PKCS1v15 for signatures:
/// - Perl: `use_pkcs1_oaep_padding` is a no-op for sign/verify (OAEP is encryption-only)
/// - Go: `rsa.VerifyPKCS1v15` explicitly
/// - JS: `crypto.createVerify('sha256')` defaults to PKCS1v15
pub fn verify_rsa_sha256(cert_pem: &str, message: &[u8], signature_base64: &str) -> Result<bool> {
    use openssl::hash::MessageDigest;
    use openssl::sign::Verifier;
    use openssl::x509::X509;

    // Parse the certificate to extract the public key
    let cert_pem_trimmed = cert_pem.trim();
    let x509 = X509::from_pem(cert_pem_trimmed.as_bytes())
        .map_err(|e| anyhow!("Failed to parse certificate PEM: {}", e))?;
    let pubkey = x509
        .public_key()
        .map_err(|e| anyhow!("Failed to extract public key: {}", e))?;

    // Decode the base64 signature (handle both line-wrapped and single-line)
    let sig_clean: String = signature_base64
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let signature = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &sig_clean)
        .map_err(|e| {
            anyhow!(
                "Failed to decode signature base64 ({} chars): {}",
                sig_clean.len(),
                e
            )
        })?;

    // Verify with PKCS1v15 SHA256
    let mut verifier = Verifier::new(MessageDigest::sha256(), &pubkey)
        .map_err(|e| anyhow!("Failed to create verifier: {}", e))?;
    verifier
        .set_rsa_padding(openssl::rsa::Padding::PKCS1)
        .map_err(|e| anyhow!("Failed to set padding: {}", e))?;
    verifier.update(message)?;

    match verifier.verify(&signature) {
        Ok(valid) => Ok(valid),
        Err(e) => {
            // OpenSSL verify error — this means the signature format was valid
            // but the content didn't match
            Err(anyhow!(
                "RSA verify error (sig {} bytes, msg {} bytes): {}",
                signature.len(),
                message.len(),
                e
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint_format() {
        // A minimal DER cert for testing format
        let fake_der = b"test certificate data";
        let fp = cert_sha1_fingerprint(fake_der);

        // Should be uppercase hex, colon-separated
        assert!(fp.contains(':'));
        assert_eq!(fp.len(), 59); // 20 bytes * 3 - 1 (colons between pairs)
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit() || c == ':'));
        // All hex should be uppercase
        assert!(fp
            .chars()
            .filter(|c| c.is_ascii_alphabetic())
            .all(|c| c.is_ascii_uppercase()));
    }

    #[test]
    fn test_pem_to_der() {
        let pem = "-----BEGIN CERTIFICATE-----\nAQIDBA==\n-----END CERTIFICATE-----\n";
        let der = pem_to_der(pem).unwrap();
        assert_eq!(der, vec![1, 2, 3, 4]);
    }

    /// Verify fingerprint against dev cert.
    /// Run with: cargo test -- --ignored test_fingerprint_of_dev_cert
    #[test]
    #[ignore]
    fn test_fingerprint_of_dev_cert() {
        let cert_path = format!(
            "{}/GT/backplane/devkeys/dev.crt",
            std::env::var("HOME").unwrap_or_default()
        );
        let cert_pem = std::fs::read_to_string(&cert_path)
            .unwrap_or_else(|_| panic!("Dev cert not found at {}", cert_path));
        let fp = cert_pem_fingerprint(&cert_pem).unwrap();
        assert_eq!(
            fp, "BC:6E:86:C2:46:44:F7:DC:7F:1D:17:89:D1:9A:E5:09:E4:08:8B:B0",
            "Dev cert fingerprint mismatch"
        );
    }
}

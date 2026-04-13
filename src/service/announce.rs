//! Announcement packet building and signing.
//!
//! Generates v3+v4 announcement packets matching Perl Announcer.pm `_build_packet`.
//! The v3 wrapper array includes a v4 extension hash with RLE-encoded action vectors
//! appended to the envelopes array (Perl Announcer.pm:187).

use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashMap;

use super::handler::ActionInfo;

/// Perl Announcer.pm:103
#[allow(dead_code)]
const ANNOUNCEABLE: &[&str] = &["create", "destroy", "noauth", "read", "secret", "update"];

/// Build a v3+v4 announcement packet (signed, ready for zlib + multicast).
///
/// Format: `json_blob\n\ncert_pem\nbase64_sig(76-char wrapped)\n`
/// JSON: `[3, ident, sector, weight, interval_ms, uri, [envelopes..., v4_hash], v3_actions, ts]`
///
/// Perl Announcer.pm:122-204
pub fn build_announcement_packet(
    identity: &str,
    sector: &str,
    envelopes: &[String],
    uri: &str,
    actions: &[ActionInfo],
    key_pem: &[u8],
    cert_pem: &[u8],
    weight: u32,
    interval_secs: u32,
    active: bool,
) -> Result<Vec<u8>> {
    let cert_pem_str = std::str::from_utf8(cert_pem)?;

    // Build v3 action classes and v4 extension vectors
    // Perl Announcer.pm:127-157
    let mut v3_class_map: HashMap<String, Vec<Value>> = HashMap::new();
    let v4_acns: Vec<String> = Vec::new();
    let v4_acname: Vec<String> = Vec::new();
    let v4_acver: Vec<u32> = Vec::new();
    let v4_acflag: Vec<String> = Vec::new();
    let v4_acsec: Vec<String> = Vec::new();
    let v4_acenv: Vec<String> = Vec::new();

    if active {
        for action in actions {
            let (namespace, method) = match action.name.rsplit_once('.') {
                Some((ns, m)) => (ns, m),
                None => continue,
            };

            // Filter flags to announceable set, sorted alphabetically
            // Perl Announcer.pm:137-139
            let flags = String::new(); // TODO: action flags when ActionInfo gains them

            // All actions go into v3 compat zone (no custom sector/envelopes yet)
            // Perl Announcer.pm:141-148
            let cls = v3_class_map.entry(namespace.to_string()).or_default();
            let mut action_arr = vec![
                Value::String(method.to_string()),
                Value::String(flags),
            ];
            if action.version != 1 {
                action_arr.push(json!(action.version));
            }
            cls.push(Value::Array(action_arr));
        }
    }

    let mut v3_classes: Vec<Value> = Vec::new();
    for (namespace, methods) in &v3_class_map {
        let mut cls = vec![Value::String(namespace.clone())];
        cls.extend(methods.iter().cloned());
        v3_classes.push(Value::Array(cls));
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    let effective_weight = if active { weight } else { 0 };
    let interval_ms = interval_secs as u64 * 1000;

    // Build v4 extension hash — Perl Announcer.pm:159-175
    let v4_hash = json!({
        "vmaj": 4,
        "vmin": 0,
        "acname": rle_encode_strings(&v4_acname),
        "acns": rle_encode_strings(&v4_acns),
        "acflag": rle_encode_strings(&v4_acflag),
        "acsec": rle_encode_strings(&v4_acsec),
        "acenv": rle_encode_strings(&v4_acenv),
        "acver": rle_encode_numbers(&v4_acver),
    });

    // Position [6]: envelopes + v4 extension hash
    // Perl Announcer.pm:187
    let mut env_plus_v4: Vec<Value> = envelopes.iter().map(|e| Value::String(e.clone())).collect();
    env_plus_v4.push(v4_hash);

    // Perl Announcer.pm:179-190: v3 format with v4 extension
    let json_array = json!([
        3,
        identity,
        sector,
        effective_weight,
        interval_ms,
        uri,
        env_plus_v4,
        if v3_classes.is_empty() { Value::Null } else { Value::Array(v3_classes) },
        timestamp,
    ]);

    let json_blob = serde_json::to_string(&json_array)?;

    // Sign with RSA SHA256 PKCS1v15 — Perl Announcer.pm:196-197
    let rsa_key = openssl::rsa::Rsa::private_key_from_pem(key_pem)?;
    let pkey = openssl::pkey::PKey::from_rsa(rsa_key)?;
    let mut signer =
        openssl::sign::Signer::new(openssl::hash::MessageDigest::sha256(), &pkey)?;
    signer.set_rsa_padding(openssl::rsa::Padding::PKCS1)?;
    signer.update(json_blob.as_bytes())?;
    let signature = signer.sign_to_vec()?;

    // Base64 encode with 76-char line wrapping to match Perl MIME::Base64
    let sig_base64 = base64_encode_wrapped(&signature, 76);

    // Perl Announcer.pm:199-201
    // Format: json\n\ncert_pem\nbase64_sig\n
    let packet = format!("{}\n\n{}\n{}\n", json_blob, cert_pem_str, sig_base64);
    Ok(packet.into_bytes())
}

/// RLE-encode a string vector. Perl Announcer.pm `__torle($list)`.
/// Single occurrence → bare value, repeated → [count, value].
fn rle_encode_strings(items: &[String]) -> Vec<Value> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < items.len() {
        let val = &items[i];
        let mut count = 1;
        while i + count < items.len() && &items[i + count] == val {
            count += 1;
        }
        if count > 1 {
            out.push(json!([count, val]));
        } else {
            out.push(Value::String(val.clone()));
        }
        i += count;
    }
    out
}

/// RLE-encode a numeric vector. Perl Announcer.pm `__torle($list, 1)`.
fn rle_encode_numbers(items: &[u32]) -> Vec<Value> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < items.len() {
        let val = items[i];
        let mut count = 1;
        while i + count < items.len() && items[i + count] == val {
            count += 1;
        }
        if count > 1 {
            out.push(json!([count, val]));
        } else {
            out.push(json!(val));
        }
        i += count;
    }
    out
}

/// Base64 encode with line wrapping at `width` characters.
/// Matches Perl MIME::Base64::encode_base64 default behavior.
fn base64_encode_wrapped(data: &[u8], width: usize) -> String {
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data);
    let mut result = String::with_capacity(encoded.len() + encoded.len() / width + 1);
    for (i, ch) in encoded.chars().enumerate() {
        if i > 0 && i % width == 0 {
            result.push('\n');
        }
        result.push(ch);
    }
    result.push('\n');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rle_encode_strings() {
        let items = vec!["a".into(), "a".into(), "b".into(), "c".into(), "c".into(), "c".into()];
        let rle = rle_encode_strings(&items);
        assert_eq!(rle, vec![json!([2, "a"]), json!("b"), json!([3, "c"])]);
    }

    #[test]
    fn test_rle_encode_strings_empty() {
        let rle = rle_encode_strings(&[]);
        assert!(rle.is_empty());
    }

    #[test]
    fn test_rle_encode_numbers() {
        let items = vec![1, 1, 1, 2, 3, 3];
        let rle = rle_encode_numbers(&items);
        assert_eq!(rle, vec![json!([3, 1]), json!(2), json!([2, 3])]);
    }

    #[test]
    fn test_base64_wrapped() {
        // 256 bytes → 344 base64 chars → should wrap at 76
        let data = vec![0xABu8; 256];
        let encoded = base64_encode_wrapped(&data, 76);
        for line in encoded.trim_end().split('\n') {
            assert!(line.len() <= 76, "line too long: {} chars", line.len());
        }
        assert!(encoded.ends_with('\n'));
    }

    /// Build a packet and verify it can be parsed by our own announcement parser.
    #[test]
    #[ignore] // requires dev keypair
    fn test_roundtrip_announcement() {
        let home = std::env::var("HOME").unwrap_or_default();
        let key_pem = std::fs::read(format!("{}/GT/backplane/devkeys/dev.key", home)).unwrap();
        let cert_pem = std::fs::read(format!("{}/GT/backplane/devkeys/dev.crt", home)).unwrap();

        let actions = vec![
            ActionInfo { name: "ScampRsTest.echo".into(), version: 1 },
            ActionInfo { name: "ScampRsTest.health_check".into(), version: 1 },
        ];

        let packet = build_announcement_packet(
            "scamp-rs-test:abc123",
            "main",
            &["json".to_string()],
            "beepish+tls://10.0.0.1:30100",
            &actions,
            &key_pem,
            &cert_pem,
            1, 5, true,
        ).unwrap();

        let packet_str = String::from_utf8(packet).unwrap();

        // Parse with our announcement parser
        let ann = crate::discovery::packet::AnnouncementPacket::parse(&packet_str).unwrap();
        assert_eq!(ann.body.info.identity, "scamp-rs-test:abc123");
        assert_eq!(ann.body.info.uri, "beepish+tls://10.0.0.1:30100");
        assert_eq!(ann.body.params.weight, 1);
        assert_eq!(ann.body.params.interval, 5000);
        assert!(!ann.body.actions.is_empty());

        // Verify signature
        assert!(ann.signature_is_valid(), "Signature should be valid");

        // Verify v4 extension hash is present
        // The envelopes array should contain "json" and a v4 hash object
        let json_val: serde_json::Value = serde_json::from_str(&ann.json_blob).unwrap();
        let env_array = json_val.as_array().unwrap()[6].as_array().unwrap();
        assert!(env_array.iter().any(|v| v.is_object()), "Should contain v4 extension hash");
    }
}

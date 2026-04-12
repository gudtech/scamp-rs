//! Announcement packet building and signing.
//!
//! Generates v3 announcement packets matching Perl Announcer.pm `_build_packet`.

use anyhow::{anyhow, Result};
use std::collections::HashMap;

use super::handler::RegisteredAction;

/// Build a v3 announcement packet (signed, ready for multicast or cache).
///
/// Format: `json_blob\n\ncert_pem\n\nbase64_sig\n`
/// JSON: `[3, ident, sector, weight, interval_ms, uri, [envelopes...], v3_actions, timestamp]`
pub fn build_announcement_packet(
    identity: &str,
    sector: &str,
    envelopes: &[String],
    uri: &str,
    actions: &HashMap<String, RegisteredAction>,
    key_pem: &[u8],
    cert_pem: &[u8],
) -> Result<String> {
    let cert_pem_str = std::str::from_utf8(cert_pem)?;

    // Build v3 action list: [[ClassName, [actionName, flags, version?], ...], ...]
    let mut class_map: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    for (_key, registered) in actions {
        let action_name = &registered.name;
        let parts: Vec<&str> = action_name.rsplitn(2, '.').collect();
        if parts.len() != 2 {
            continue;
        }
        let method = parts[0];
        let namespace = parts[1];

        let entry = class_map.entry(namespace.to_string()).or_default();
        let mut action_arr = vec![
            serde_json::Value::String(method.to_string()),
            serde_json::Value::String(String::new()), // flags
        ];
        if registered.version != 1 {
            action_arr.push(serde_json::Value::Number(registered.version.into()));
        }
        entry.push(serde_json::Value::Array(action_arr));
    }

    let mut v3_classes: Vec<serde_json::Value> = Vec::new();
    for (namespace, actions) in &class_map {
        let mut cls = vec![serde_json::Value::String(namespace.clone())];
        cls.extend(actions.iter().cloned());
        v3_classes.push(serde_json::Value::Array(cls));
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    // Perl Announcer.pm:179-190: v3 format
    let json_array = serde_json::json!([
        3,
        identity,
        sector,
        1,    // weight
        5000, // interval in milliseconds (Perl Announcer.pm:163)
        uri,
        envelopes,
        v3_classes,
        timestamp,
    ]);

    let json_blob = serde_json::to_string(&json_array)?;

    // Sign with RSA SHA256 PKCS1v15 (Perl Announcer.pm:196-197)
    let rsa_key = openssl::rsa::Rsa::private_key_from_pem(key_pem)?;
    let pkey = openssl::pkey::PKey::from_rsa(rsa_key)?;
    let mut signer =
        openssl::sign::Signer::new(openssl::hash::MessageDigest::sha256(), &pkey)?;
    signer.set_rsa_padding(openssl::rsa::Padding::PKCS1)?;
    signer.update(json_blob.as_bytes())?;
    let signature = signer.sign_to_vec()?;
    let sig_base64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &signature);

    // Format: json\n\ncert_pem\n\nbase64_sig\n
    // Perl Announcer.pm:198-201
    let cert_str = cert_pem_str.trim_end_matches('\n');
    Ok(format!("{}\n\n{}\n\n{}\n", json_blob, cert_str, sig_base64))
}

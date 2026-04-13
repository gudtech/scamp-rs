//! Authorization table checking — JS ticket.js:51-95, C# Ticket.cs:85-117.
//!
//! Fetches the authz table from `Auth.getAuthzTable~1`, caches for 5 minutes,
//! and checks whether a ticket holder has the required privileges for an action.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use tokio::sync::RwLock;

use super::ticket::Ticket;
use crate::requester::Requester;

/// Cache TTL for the authorization table — JS ticket.js:53
const AUTHZ_CACHE_TTL_SECS: u64 = 300;

struct CachedAuthzTable {
    /// Action name (lowercase) → required privilege IDs
    table: HashMap<String, Vec<u64>>,
    expires_at: u64,
}

/// Checks whether a ticket grants access to a specific action.
/// Fetches the authz table from the Auth service via SCAMP and caches it.
pub struct AuthzChecker {
    requester: Arc<Requester>,
    ticket_verify_key: Vec<u8>,
    cached: RwLock<Option<CachedAuthzTable>>,
}

impl AuthzChecker {
    /// Create a new AuthzChecker.
    /// `ticket_verify_key`: PEM-encoded public key for ticket signature verification.
    pub fn new(requester: Arc<Requester>, ticket_verify_key: Vec<u8>) -> Self {
        AuthzChecker {
            requester,
            ticket_verify_key,
            cached: RwLock::new(None),
        }
    }

    /// Verify ticket and check privileges for the given action.
    /// JS ticket.js:71-93 (checkAccess).
    /// Returns Ok(ticket) on success, Err on invalid ticket or missing privileges.
    pub async fn check_access(
        &self,
        action: &str,
        ticket_str: &str,
    ) -> Result<Ticket> {
        // 1. Verify ticket signature and expiry
        let ticket = Ticket::verify(ticket_str, &self.ticket_verify_key)?;

        // 2. Fetch or use cached authz table
        let table = self.get_table().await?;

        // 3. Look up required privileges for this action
        let key = action.to_lowercase();
        if let Some(required_privs) = table.get(&key) {
            for &priv_id in required_privs {
                if !ticket.has_privilege(priv_id) {
                    return Err(anyhow!(
                        "Missing required privilege {} for action {}",
                        priv_id, action
                    ));
                }
            }
        }
        // If action not in table, no specific privileges required

        Ok(ticket)
    }

    /// Get the authz table, fetching from Auth service if cache is stale.
    async fn get_table(&self) -> Result<HashMap<String, Vec<u64>>> {
        // Check cache
        {
            let cached = self.cached.read().await;
            if let Some(ct) = &*cached {
                if now_secs() < ct.expires_at {
                    return Ok(ct.table.clone());
                }
            }
        }

        // Fetch fresh table from Auth.getAuthzTable~1
        // JS ticket.js:55-68
        let resp = self
            .requester
            .request("Auth.getAuthzTable", 1, b"{}".to_vec())
            .await?;

        let table = parse_authz_response(&resp.body)?;

        // Cache for 5 minutes
        let mut cached = self.cached.write().await;
        *cached = Some(CachedAuthzTable {
            table: table.clone(),
            expires_at: now_secs() + AUTHZ_CACHE_TTL_SECS,
        });

        Ok(table)
    }
}

/// Parse the Auth.getAuthzTable response into action → privilege IDs.
/// Response format: `{"action.name": [priv_id, ...], "_NAMES": [...], ...}`
/// JS ticket.js:60-67
fn parse_authz_response(body: &[u8]) -> Result<HashMap<String, Vec<u64>>> {
    let json: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| anyhow!("Invalid authz table JSON: {}", e))?;

    let obj = json
        .as_object()
        .ok_or_else(|| anyhow!("Authz table is not a JSON object"))?;

    let mut table = HashMap::new();
    for (key, value) in obj {
        // Skip _NAMES and other metadata keys
        if key.starts_with('_') {
            continue;
        }
        if let Some(arr) = value.as_array() {
            let privs: Vec<u64> = arr
                .iter()
                .filter_map(|v| v.as_u64())
                .collect();
            table.insert(key.to_lowercase(), privs);
        }
    }
    Ok(table)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_authz_response() {
        let body = br#"{
            "auth.getauthztable": [1, 2],
            "product.sku.fetch": [3],
            "public.action": [],
            "_NAMES": [null, "admin", "read", "product_read"]
        }"#;

        let table = parse_authz_response(body).unwrap();
        assert_eq!(table.get("auth.getauthztable"), Some(&vec![1, 2]));
        assert_eq!(table.get("product.sku.fetch"), Some(&vec![3]));
        assert_eq!(table.get("public.action"), Some(&vec![]));
        assert!(table.get("_NAMES").is_none(), "_NAMES should be skipped");
    }

    #[test]
    fn test_parse_authz_response_empty() {
        let table = parse_authz_response(b"{}").unwrap();
        assert!(table.is_empty());
    }
}

//! Authorization checking — verifies tickets and checks action privileges.
//!
//! Three modes:
//! - **Remote** (production): Fetches authz table from `Auth.getAuthzTable~1` via SCAMP RPC
//! - **Static** (tests/standalone): In-memory table, no network required
//! - **File** (standalone): Loads authz table from a JSON file on disk
//!
//! All modes verify ticket signatures and enforce deny-by-default for unconfigured actions.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use tokio::sync::RwLock;

use super::ticket::Ticket;
use crate::requester::Requester;

/// Cache TTL for the remote authorization table — JS ticket.js:53
const AUTHZ_CACHE_TTL_SECS: u64 = 300;

/// Action name (lowercase) → required privilege IDs.
pub type AuthzTable = HashMap<String, Vec<u64>>;

/// Authorization checker — verifies tickets and checks action privileges.
/// Abstracts over the authz data source (remote Auth service, static table, or file).
pub struct AuthzChecker {
    ticket_verify_key: Vec<u8>,
    source: AuthzSource,
}

enum AuthzSource {
    /// Production: fetch from Auth.getAuthzTable~1, cache for 5 minutes
    Remote {
        requester: Arc<Requester>,
        cached: RwLock<Option<CachedAuthzTable>>,
    },
    /// Tests/standalone: static in-memory table
    Static { table: AuthzTable },
}

struct CachedAuthzTable {
    table: AuthzTable,
    expires_at: u64,
}

impl AuthzChecker {
    /// Create an AuthzChecker that fetches the authz table from the Auth service.
    /// Used in production (GudTech/RetailOps).
    pub fn from_requester(requester: Arc<Requester>, ticket_verify_key: Vec<u8>) -> Self {
        AuthzChecker {
            ticket_verify_key,
            source: AuthzSource::Remote {
                requester,
                cached: RwLock::new(None),
            },
        }
    }

    /// Create an AuthzChecker with a static authz table.
    /// Used in tests and standalone deployments.
    pub fn from_table(table: AuthzTable, ticket_verify_key: Vec<u8>) -> Self {
        AuthzChecker {
            ticket_verify_key,
            source: AuthzSource::Static { table },
        }
    }

    /// Create an AuthzChecker that loads the authz table from a JSON file.
    /// File format: `{"action.name": [priv_id, ...], ...}`
    pub fn from_file(path: &str, ticket_verify_key: Vec<u8>) -> Result<Self> {
        let content = std::fs::read(path)?;
        let table = parse_authz_response(&content)?;
        Ok(Self::from_table(table, ticket_verify_key))
    }

    /// Verify ticket and check privileges for the given action.
    /// Deny by default: unconfigured actions are rejected.
    pub async fn check_access(&self, action: &str, ticket_str: &str) -> Result<Ticket> {
        let ticket = Ticket::verify(ticket_str, &self.ticket_verify_key)?;
        let table = self.get_table().await?;

        let key = action.to_lowercase();
        match table.get(&key) {
            Some(required_privs) => {
                for &priv_id in required_privs {
                    if !ticket.has_privilege(priv_id) {
                        return Err(anyhow!("Missing required privilege {} for action {}", priv_id, action));
                    }
                }
            }
            None => {
                return Err(anyhow!("Unconfigured action: {}", action));
            }
        }

        Ok(ticket)
    }

    async fn get_table(&self) -> Result<AuthzTable> {
        match &self.source {
            AuthzSource::Static { table } => Ok(table.clone()),
            AuthzSource::Remote { requester, cached } => {
                // Check cache
                {
                    let c = cached.read().await;
                    if let Some(ct) = &*c {
                        if now_secs() < ct.expires_at {
                            return Ok(ct.table.clone());
                        }
                    }
                }
                // Fetch from Auth.getAuthzTable~1 — JS ticket.js:55-68
                let resp = requester.request("Auth.getAuthzTable", 1, b"{}".to_vec()).await?;
                let table = parse_authz_response(&resp.body)?;
                let mut c = cached.write().await;
                *c = Some(CachedAuthzTable {
                    table: table.clone(),
                    expires_at: now_secs() + AUTHZ_CACHE_TTL_SECS,
                });
                Ok(table)
            }
        }
    }
}

/// Parse the Auth.getAuthzTable response (or file) into action → privilege IDs.
/// Format: `{"action.name": [priv_id, ...], "_NAMES": [...], ...}`
pub fn parse_authz_response(body: &[u8]) -> Result<AuthzTable> {
    let json: serde_json::Value = serde_json::from_slice(body).map_err(|e| anyhow!("Invalid authz table JSON: {}", e))?;
    let obj = json.as_object().ok_or_else(|| anyhow!("Authz table is not a JSON object"))?;

    let mut table = HashMap::new();
    for (key, value) in obj {
        if key.starts_with('_') {
            continue;
        }
        if let Some(arr) = value.as_array() {
            let privs: Vec<u64> = arr.iter().filter_map(|v| v.as_u64()).collect();
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

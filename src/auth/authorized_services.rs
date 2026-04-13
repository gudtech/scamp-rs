//! Authorized services file parsing and action authorization.
//!
//! Matches Perl `ServiceInfo.pm:111-168` and JS `handle/service.js:168-219`.
//!
//! File format (one entry per line):
//!   `FINGERPRINT token1, token2, ...`
//!
//! Token rules:
//! - Tokens are comma-separated
//! - If a token contains `:`, `:ALL` is replaced with `:.*`
//! - If a token has no `:`, `main:` is prefixed
//! - Each token is regex-escaped
//! - Combined into: `/^(?:tok1|tok2)(?:\.|$)/i`
//!
//! Special cases:
//! - `_meta.*` actions are always authorized (Perl ServiceInfo.pm:147)
//! - Actions or sectors containing `:` are rejected (Perl ServiceInfo.pm:149)

use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::time::SystemTime;

/// Authorized services registry. Maps certificate fingerprints to action patterns.
pub struct AuthorizedServices {
    entries: HashMap<String, AuthEntry>,
    file_path: String,
    last_mtime: Option<SystemTime>,
}

struct AuthEntry {
    /// Regex patterns compiled from the token list.
    /// Each pattern matches `^(?:tok)(?:\.|$)` case-insensitively.
    patterns: Vec<regex::Regex>,
}

impl AuthorizedServices {
    /// Load authorized services from a file.
    pub fn load(path: &str) -> Result<Self> {
        let mut svc = AuthorizedServices {
            entries: HashMap::new(),
            file_path: path.to_string(),
            last_mtime: None,
        };
        svc.reload()?;
        Ok(svc)
    }

    /// Create an empty (allow-nothing) authorized services.
    pub fn empty() -> Self {
        AuthorizedServices {
            entries: HashMap::new(),
            file_path: String::new(),
            last_mtime: None,
        }
    }

    /// Reload if the file has been modified (hot-reload).
    /// Matches Perl ServiceInfo.pm:117-118 mtime check.
    pub fn reload_if_changed(&mut self) -> Result<bool> {
        if self.file_path.is_empty() {
            return Ok(false);
        }
        let metadata = fs::metadata(&self.file_path)?;
        let mtime = metadata.modified()?;
        if self.last_mtime == Some(mtime) {
            return Ok(false);
        }
        self.reload()?;
        Ok(true)
    }

    fn reload(&mut self) -> Result<()> {
        if self.file_path.is_empty() {
            return Ok(());
        }
        let content = fs::read_to_string(&self.file_path)?;
        let metadata = fs::metadata(&self.file_path)?;
        self.last_mtime = Some(metadata.modified()?);
        self.parse_content(&content);
        Ok(())
    }

    /// Parse authorized_services content into entries.
    /// Used by reload() and tests.
    fn parse_content(&mut self, content: &str) {
        self.entries.clear();

        for line in content.lines() {
            // Strip comments — Perl ServiceInfo.pm:125-126
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }

            // Perl: my ($fingerprint, $toks) = $line =~ /^(\S*)\s*(.*)$/;
            let (fingerprint, tokens_str) = match line.split_once(char::is_whitespace) {
                Some((fp, rest)) => (fp.trim(), rest.trim()),
                None => continue,
            };

            // Perl ServiceInfo.pm:130: my @toks = map { quotemeta } split /\s*,\s*/, $toks;
            let patterns: Vec<regex::Regex> = tokens_str
                .split(',')
                .map(|t| t.trim())
                .filter(|t| !t.is_empty())
                .filter_map(|token| {
                    let escaped = regex::escape(token);
                    // Perl ServiceInfo.pm:131-132
                    let pattern = if escaped.contains(':') {
                        escaped.replace(":ALL", ":.*")
                    } else {
                        format!("main:{}", escaped)
                    };

                    // Build full regex: /^(?:pattern)(?:\.|$)/i
                    // Perl ServiceInfo.pm:135
                    let rx = format!("(?i)^(?:{})(?:\\.|$)", pattern);
                    regex::Regex::new(&rx).ok()
                })
                .collect();

            self.entries.insert(
                fingerprint.to_string(),
                AuthEntry { patterns },
            );
        }
    }

    /// Check if an action is authorized for a given fingerprint.
    /// Matches Perl ServiceInfo.pm:141-167.
    pub fn is_authorized(&self, fingerprint: &str, sector: &str, action: &str) -> bool {
        // _meta.* actions are always authorized (Perl ServiceInfo.pm:147)
        if action.starts_with("_meta.") {
            return true;
        }

        // Reject if sector or action contains ':' (Perl ServiceInfo.pm:149)
        if sector.contains(':') || action.contains(':') {
            return false;
        }

        let entry = match self.entries.get(fingerprint) {
            Some(e) => e,
            None => return false, // Unknown fingerprint → not authorized
        };

        // Match "sector:action" against patterns
        // Perl ServiceInfo.pm:163: return 1 if "$sector:$action" =~ /$rx/;
        let check = format!("{}:{}", sector, action);
        entry.patterns.iter().any(|rx| rx.is_match(&check))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_auth(content: &str) -> AuthorizedServices {
        let mut svc = AuthorizedServices::empty();
        svc.parse_content(content);
        svc
    }

    #[test]
    fn test_all_sectors_authorized() {
        // Matches the real dev authorized_services file format
        let auth = make_auth(
            "BC:6E:86 background:ALL, main:ALL, web:ALL",
        );
        assert!(auth.is_authorized("BC:6E:86", "main", "Product.Sku.fetch"));
        assert!(auth.is_authorized("BC:6E:86", "background", "Worker.process"));
        assert!(auth.is_authorized("BC:6E:86", "web", "Page.render"));
    }

    #[test]
    fn test_no_colon_defaults_to_main() {
        let auth = make_auth("FP:AA:BB Product.Sku");
        assert!(auth.is_authorized("FP:AA:BB", "main", "Product.Sku.fetch"));
        assert!(!auth.is_authorized("FP:AA:BB", "web", "Product.Sku.fetch"));
    }

    #[test]
    fn test_meta_always_authorized() {
        let auth = make_auth("FP:AA:BB SomeAction");
        // _meta.* should be authorized even without explicit pattern
        assert!(auth.is_authorized("UNKNOWN:FP", "main", "_meta.documentation"));
    }

    #[test]
    fn test_unknown_fingerprint_denied() {
        let auth = make_auth("FP:AA:BB main:ALL");
        assert!(!auth.is_authorized("OTHER:FP", "main", "Product.Sku.fetch"));
    }

    #[test]
    fn test_colon_in_sector_rejected() {
        let auth = make_auth("FP:AA:BB main:ALL");
        assert!(!auth.is_authorized("FP:AA:BB", "evil:sector", "Action"));
    }

    #[test]
    fn test_colon_in_action_rejected() {
        let auth = make_auth("FP:AA:BB main:ALL");
        assert!(!auth.is_authorized("FP:AA:BB", "main", "evil:action"));
    }

    #[test]
    fn test_case_insensitive() {
        let auth = make_auth("FP:AA:BB Product.Sku");
        assert!(auth.is_authorized("FP:AA:BB", "main", "product.sku.fetch"));
        assert!(auth.is_authorized("FP:AA:BB", "main", "PRODUCT.SKU.fetch"));
    }

    #[test]
    fn test_comment_handling() {
        let auth = make_auth(
            "# This is a comment\nFP:AA:BB main:ALL # inline comment",
        );
        assert!(auth.is_authorized("FP:AA:BB", "main", "Action.test"));
    }

    #[test]
    fn test_prefix_match_with_dot_boundary() {
        // The pattern should match at dot boundaries: `(?:\.|$)`
        let auth = make_auth("FP:AA:BB Product");
        assert!(auth.is_authorized("FP:AA:BB", "main", "Product.Sku.fetch"));
        assert!(auth.is_authorized("FP:AA:BB", "main", "Product"));
        // Should NOT match ProductExtra (no dot boundary)
        assert!(!auth.is_authorized("FP:AA:BB", "main", "ProductExtra.fetch"));
    }

    #[test]
    #[ignore] // requires live dev environment
    fn test_load_real_authorized_services() {
        let home = std::env::var("HOME").unwrap_or_default();
        let path = format!("{}/GT/backplane/etc/authorized_services", home);
        let auth = AuthorizedServices::load(&path).expect("Failed to load authorized_services");

        // The dev cert should be authorized for main sector
        let dev_fp = "BC:6E:86:C2:46:44:F7:DC:7F:1D:17:89:D1:9A:E5:09:E4:08:8B:B0";
        assert!(
            auth.is_authorized(dev_fp, "main", "API.Status.health_check"),
            "Dev cert should be authorized for main:API.Status.health_check"
        );
        assert!(
            auth.is_authorized(dev_fp, "web", "Page.render"),
            "Dev cert should be authorized for web sector"
        );
        assert!(
            !auth.is_authorized("UNKNOWN:FP", "main", "API.Status.health_check"),
            "Unknown fingerprint should not be authorized"
        );
    }
}

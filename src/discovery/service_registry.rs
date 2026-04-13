use std::collections::{BTreeMap, HashMap};
use std::fs::File;

use anyhow::Result;

use crate::auth::authorized_services::AuthorizedServices;
use crate::config::Config;

use super::{
    cache_file::CacheFileAnnouncementIterator,
    packet::AnnouncementPacket,
    service_info::{Action, AnnouncementParams, CrudOp, Flag, ServiceInfo},
};

pub struct ActionEntry {
    pub action: Action,
    pub service_info: ServiceInfo,
    pub announcement_params: AnnouncementParams,
    pub authorized: bool,
}

impl ActionEntry {
    /// Per-action timeout in seconds from `t600` flags, with +5s padding.
    /// Perl ServiceInfo.pm:257-258: `timeout = $timeout + 5`
    /// Returns None if no timeout flag is set (use default RPC timeout).
    pub fn timeout_secs(&self) -> Option<u64> {
        self.action.flags.iter().find_map(|f| match f {
            Flag::Timeout(secs) => Some(*secs as u64 + 5),
            _ => None,
        })
    }
}

/// Index key format: `sector:namespace.action.vVERSION` (lowercased)
/// Matches Perl ServiceInfo.pm:188 and JS serviceMgr.js:221
fn make_index_key(sector: &str, action_path: &str, version: u32) -> String {
    format!("{}:{}.v{}", sector, action_path, version).to_lowercase()
}

/// Make CRUD alias key: `sector:namespace._tag.vVERSION`
/// Perl ServiceInfo.pm:191-192, JS serviceMgr.js:223-225
fn make_crud_alias_key(sector: &str, namespace: &str, tag: &str, version: u32) -> String {
    format!("{}:{}._{}.v{}", sector, namespace, tag, version).to_lowercase()
}

pub struct ServiceRegistry {
    /// Primary index: sector:action.vVERSION → Vec<ActionEntry>
    actions_by_key: BTreeMap<String, Vec<ActionEntry>>,
    /// Track latest timestamp per service identity for replay protection (D9).
    /// Key: `fingerprint identity` — Perl ServiceManager.pm:29
    seen_timestamps: HashMap<String, f64>,
}

impl ServiceRegistry {
    pub fn new_from_cache(config: &Config) -> Result<Self> {
        let mut actions_by_key: BTreeMap<String, Vec<ActionEntry>> = BTreeMap::new();
        let mut seen_timestamps: HashMap<String, f64> = HashMap::new();

        let cache_path: String = match config.get("discovery.cache_path") {
            Some(Ok(path)) => path,
            Some(Err(e)) => return Err(anyhow::anyhow!("Failed to get cache path: {}", e)),
            None => return Err(anyhow::anyhow!("No cache path found")),
        };

        // Load authorized_services (Perl ServiceInfo.pm:112-113)
        let auth = match config.get::<String>("bus.authorized_services") {
            Some(Ok(path)) => match AuthorizedServices::load(&path) {
                Ok(a) => a,
                Err(e) => {
                    log::warn!("Failed to load authorized_services from {}: {}", path, e);
                    AuthorizedServices::empty()
                }
            },
            _ => {
                log::debug!("No bus.authorized_services configured");
                AuthorizedServices::empty()
            }
        };

        let mut file = File::open(&cache_path).map_err(|e| {
            anyhow::anyhow!("Failed to open discovery cache file {}, {}", cache_path, e)
        })?;

        // D7: Cache staleness check — Perl ServiceManager.pm:83-88
        let cache_max_age: u64 = config
            .get::<u64>("discovery.cache_max_age")
            .and_then(|r| r.ok())
            .unwrap_or(120);
        if let Ok(metadata) = file.metadata() {
            if let Ok(modified) = metadata.modified() {
                let age = modified.elapsed().unwrap_or_default();
                if age.as_secs() > cache_max_age {
                    log::warn!(
                        "Discovery cache is stale ({:.0}s old, max {}s): {}",
                        age.as_secs(), cache_max_age, cache_path
                    );
                }
            }
        }

        let iterator = CacheFileAnnouncementIterator::new(&mut file);
        for announcement_packet in iterator {
            let packet = announcement_packet?;
            if !packet.signature_is_valid() {
                log::debug!("Skipping announcement with invalid signature: {}", packet.body.info.identity);
                continue;
            }

            let AnnouncementPacket { body, .. } = packet;

            let fingerprint = body.info.fingerprint.as_deref().unwrap_or("");

            // D8: Announcement TTL/expiry — Perl ServiceManager.pm:38
            // expires = timestamp + send_interval_secs * 2.1
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();
            let interval_secs = body.params.interval as f64 / 1000.0;
            let expires = body.params.timestamp + interval_secs * 2.1;
            if now > expires {
                log::debug!("Skipping expired announcement for {} (expired {:.0}s ago)",
                    body.info.identity, now - expires);
                continue;
            }

            // D26: Service deduplication by fingerprint+identity
            // D9: Timestamp replay protection — reject older timestamps
            // Perl ServiceManager.pm:29-35
            let dedup_key = format!("{} {}", fingerprint, body.info.identity);
            let timestamp = body.params.timestamp;
            if let Some(&prev_ts) = seen_timestamps.get(&dedup_key) {
                if timestamp <= prev_ts {
                    log::debug!("Skipping stale announcement for {} (ts {} <= {})", body.info.identity, timestamp, prev_ts);
                    continue;
                }
                // Newer timestamp: remove old actions for this service
                for entries in actions_by_key.values_mut() {
                    entries.retain(|e| {
                        !(e.service_info.identity == body.info.identity
                            && e.service_info.fingerprint.as_deref() == Some(fingerprint))
                    });
                }
            }
            seen_timestamps.insert(dedup_key, timestamp);

            for action in &body.actions {
                // Check authorization (Perl ServiceInfo.pm:141-167)
                let authorized = auth.is_authorized(
                    fingerprint,
                    &action.sector,
                    &action.path,
                );

                let entry = ActionEntry {
                    service_info: body.info.clone(),
                    announcement_params: body.params.clone(),
                    action: action.clone(),
                    authorized,
                };

                // Primary key: sector:action.vVERSION
                let key = make_index_key(&action.sector, &action.path, action.version);
                actions_by_key.entry(key).or_default().push(entry);

                // CRUD tag aliases (Perl ServiceInfo.pm:191-192, JS serviceMgr.js:223-225)
                let namespace = action
                    .path
                    .rsplit_once('.')
                    .map(|(ns, _)| ns)
                    .unwrap_or(&action.path);
                for flag in &action.flags {
                    if let Flag::CrudOp(op) = flag {
                        let tag = match op {
                            CrudOp::Create => "create",
                            CrudOp::Read => "read",
                            CrudOp::Update => "update",
                            CrudOp::Delete => "destroy",
                        };
                        let alias_key =
                            make_crud_alias_key(&action.sector, namespace, tag, action.version);
                        let alias_entry = ActionEntry {
                            service_info: body.info.clone(),
                            announcement_params: body.params.clone(),
                            action: action.clone(),
                            authorized: true,
                        };
                        actions_by_key.entry(alias_key).or_default().push(alias_entry);
                    }
                }
            }
        }

        Ok(Self { actions_by_key, seen_timestamps })
    }

    pub fn empty() -> Self {
        Self {
            actions_by_key: BTreeMap::new(),
            seen_timestamps: HashMap::new(),
        }
    }

    /// Returns an iterator over all actions presently in the registry
    pub fn actions_iter(&self) -> impl Iterator<Item = &ActionEntry> + '_ {
        self.actions_by_key
            .values()
            .flat_map(|entries| entries.iter())
    }

    /// Find all action entries matching the key.
    /// Key format: `sector:action.vVERSION` (lowercased)
    pub fn find_actions(&self, key: &str) -> Option<Vec<&ActionEntry>> {
        self.actions_by_key
            .get(&key.to_lowercase())
            .map(|entries| entries.iter().collect())
    }

    /// Find a random action entry matching the key.
    /// Excludes weight=0 services and unauthorized actions.
    pub fn get_action(&self, key: &str) -> Option<&ActionEntry> {
        self.actions_by_key
            .get(&key.to_lowercase())
            .and_then(|entries| {
                let active: Vec<_> = entries
                    .iter()
                    .filter(|e| e.announcement_params.weight > 0 && e.authorized)
                    .collect();
                if active.is_empty() {
                    None
                } else {
                    let random_index = rand::random::<usize>() % active.len();
                    Some(active[random_index])
                }
            })
    }

    /// Convenience: find action by sector, action name, and version.
    pub fn find_action(
        &self,
        sector: &str,
        action: &str,
        version: u32,
    ) -> Option<&ActionEntry> {
        let key = make_index_key(sector, action, version);
        self.get_action(&key)
    }

    /// Convenience: find action by sector, action name, version, filtering by envelope.
    /// Matches Perl ServiceInfo.pm:254: grep { $_ eq $envelope } @{$blk->[4]}
    pub fn find_action_with_envelope(
        &self,
        sector: &str,
        action: &str,
        version: u32,
        envelope: &str,
    ) -> Option<&ActionEntry> {
        let key = make_index_key(sector, action, version);
        self.actions_by_key
            .get(&key.to_lowercase())
            .and_then(|entries| {
                let matching: Vec<_> = entries
                    .iter()
                    .filter(|e| {
                        e.announcement_params.weight > 0
                            && e.authorized
                            && e.action.envelopes.iter().any(|env| env == envelope)
                    })
                    .collect();
                if matching.is_empty() {
                    None
                } else {
                    let random_index = rand::random::<usize>() % matching.len();
                    Some(matching[random_index])
                }
            })
    }

    /// Get the old-style pathver key (action~version) for backward compat with CLI.
    /// Converts to new key format with a given sector.
    pub fn get_action_by_pathver(&self, pathver: &str, sector: &str) -> Option<&ActionEntry> {
        // Parse action~version format
        let (action, version_str) = pathver.rsplit_once('~').unwrap_or((pathver, "1"));
        let version: u32 = version_str.parse().unwrap_or(1);
        self.find_action(sector, action, version)
    }
}

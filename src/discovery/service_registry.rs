use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::sync::Mutex;

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

/// D31/D32: Tracks failure state for a service — JS serviceMgr.js:43-52.
struct ServiceFailureState {
    /// Unix timestamps (secs) of recent failures (pruned to 24h window)
    failure_times: Vec<u64>,
    /// Don't route to this service until this time
    reactivate_at: u64,
}

pub struct ServiceRegistry {
    actions_by_key: BTreeMap<String, Vec<ActionEntry>>,
    /// Replay protection: key = `fingerprint identity` — Perl ServiceManager.pm:29
    seen_timestamps: HashMap<String, f64>,
    /// D31/D32: Failure state per service identity (interior mutability).
    failures: Mutex<HashMap<String, ServiceFailureState>>,
}

impl ServiceRegistry {
    pub fn new_from_cache(config: &Config) -> Result<Self> {
        let mut registry = Self::empty();
        registry.reload_from_cache(config)?;
        Ok(registry)
    }

    pub fn empty() -> Self {
        Self {
            actions_by_key: BTreeMap::new(),
            seen_timestamps: HashMap::new(),
            failures: Mutex::new(HashMap::new()),
        }
    }

    /// Inject a single announcement packet into the registry (D24/D25).
    /// Used by the multicast observer and cache reload.
    /// Perl ServiceManager.pm:inject()
    pub fn inject_packet(&mut self, packet: AnnouncementPacket, auth: &AuthorizedServices) {
        if !packet.signature_is_valid() {
            log::debug!(
                "Skipping announcement with invalid signature: {}",
                packet.body.info.identity
            );
            return;
        }
        let body = packet.body;
        let fingerprint = body.info.fingerprint.as_deref().unwrap_or("");

        // TTL/expiry
        let now_f = now_secs() as f64;
        let interval_secs = body.params.interval as f64 / 1000.0;
        if now_f > body.params.timestamp + interval_secs * 2.1 {
            return;
        }

        // Replay protection + dedup
        let dedup_key = format!("{} {}", fingerprint, body.info.identity);
        let timestamp = body.params.timestamp;
        if let Some(&prev_ts) = self.seen_timestamps.get(&dedup_key) {
            if timestamp <= prev_ts {
                return;
            }
            for entries in self.actions_by_key.values_mut() {
                entries.retain(|e| {
                    !(e.service_info.identity == body.info.identity
                        && e.service_info.fingerprint.as_deref() == Some(fingerprint))
                });
            }
        }
        self.seen_timestamps.insert(dedup_key, timestamp);

        for action in &body.actions {
            let authorized = auth.is_authorized(fingerprint, &action.sector, &action.path);
            let entry = ActionEntry {
                service_info: body.info.clone(),
                announcement_params: body.params.clone(),
                action: action.clone(),
                authorized,
            };
            let key = make_index_key(&action.sector, &action.path, action.version);
            self.actions_by_key.entry(key).or_default().push(entry);

            // CRUD aliases
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
                    self.actions_by_key
                        .entry(alias_key)
                        .or_default()
                        .push(alias_entry);
                }
            }
        }
    }

    /// Reload registry from the cache file (D25).
    /// Perl ServiceManager.pm:66-98.
    pub fn reload_from_cache(&mut self, config: &Config) -> Result<()> {
        let cache_path: String = config
            .get("discovery.cache_path")
            .ok_or_else(|| anyhow::anyhow!("No cache path found"))?
            .map_err(|e| anyhow::anyhow!("Failed to get cache path: {}", e))?;

        let auth = match config.get::<String>("bus.authorized_services") {
            Some(Ok(path)) => AuthorizedServices::load(&path).unwrap_or_else(|e| {
                log::warn!("Failed to load authorized_services: {}", e);
                AuthorizedServices::empty()
            }),
            _ => AuthorizedServices::empty(),
        };

        let mut file = File::open(&cache_path)
            .map_err(|e| anyhow::anyhow!("Failed to open cache {}: {}", cache_path, e))?;

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
                        "Discovery cache stale ({:.0}s old, max {}s)",
                        age.as_secs(),
                        cache_max_age
                    );
                }
            }
        }

        self.actions_by_key.clear();
        self.seen_timestamps.clear();

        for packet in CacheFileAnnouncementIterator::new(&mut file).flatten() {
            self.inject_packet(packet, &auth);
        }
        Ok(())
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
    /// D31/D32: Prefers healthy services over failed ones (JS serviceMgr.js:253-260).
    pub fn get_action(&self, key: &str) -> Option<&ActionEntry> {
        let entries = self.actions_by_key.get(&key.to_lowercase())?;
        let candidates: Vec<_> = entries
            .iter()
            .filter(|e| e.announcement_params.weight > 0 && e.authorized)
            .collect();
        self.pick_healthy(&candidates)
    }

    /// Mark a service as failed — D31/D32, JS serviceMgr.js:43-52.
    /// Exponential backoff: min(failure_count, 60) minutes.
    pub fn mark_failed(&self, identity: &str) {
        let now = now_secs();
        let mut failures = self.failures.lock().unwrap();
        let state = failures
            .entry(identity.to_string())
            .or_insert_with(|| ServiceFailureState {
                failure_times: Vec::new(),
                reactivate_at: 0,
            });
        state
            .failure_times
            .retain(|&t| t >= now.saturating_sub(86400));
        state.failure_times.push(now);
        let minutes = state.failure_times.len().min(60) as u64;
        state.reactivate_at = now + minutes * 60;
    }

    pub fn find_action(&self, sector: &str, action: &str, version: u32) -> Option<&ActionEntry> {
        self.get_action(&make_index_key(sector, action, version))
    }

    /// Find action by sector, name, version, envelope.
    /// Perl ServiceInfo.pm:254. D31/D32: Prefers healthy services.
    pub fn find_action_with_envelope(
        &self,
        sector: &str,
        action: &str,
        version: u32,
        envelope: &str,
    ) -> Option<&ActionEntry> {
        let entries = self
            .actions_by_key
            .get(&make_index_key(sector, action, version).to_lowercase())?;
        let candidates: Vec<_> = entries
            .iter()
            .filter(|e| {
                e.announcement_params.weight > 0
                    && e.authorized
                    && e.action.envelopes.iter().any(|env| env == envelope)
            })
            .collect();
        self.pick_healthy(&candidates)
    }

    /// Select a random entry, preferring healthy over failed services.
    fn pick_healthy<'a>(&self, candidates: &[&'a ActionEntry]) -> Option<&'a ActionEntry> {
        if candidates.is_empty() {
            return None;
        }
        let now = now_secs();
        let failures = self.failures.lock().unwrap();
        let (healthy, failing): (Vec<_>, Vec<_>) = candidates
            .iter()
            .partition(|e| !is_failed(&failures, &e.service_info.identity, now));
        drop(failures);
        let pool = if healthy.is_empty() {
            &failing
        } else {
            &healthy
        };
        if pool.is_empty() {
            None
        } else {
            Some(pool[rand::random::<usize>() % pool.len()])
        }
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

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Check if a service is currently marked as failed.
fn is_failed(failures: &HashMap<String, ServiceFailureState>, identity: &str, now: u64) -> bool {
    failures
        .get(identity)
        .is_some_and(|state| now < state.reactivate_at)
}

use std::{collections::BTreeMap, fs::File};

use anyhow::Result;

use crate::config::Config;

use super::{
    cache_file::CacheFileAnnouncementIterator,
    packet::AnnouncementPacket,
    service_info::{Action, AnnouncementParams, ServiceInfo},
};

pub struct ActionEntry {
    pub action: Action,
    pub service_info: ServiceInfo,
    pub announcement_params: AnnouncementParams,
    pub authorized: bool,
}

pub struct ServiceRegistry {
    actions_by_namever: BTreeMap<String, Vec<ActionEntry>>,
    // TODO - figure out how to do expiry - preferably without having to scan the whole hashmap periodically
}

impl ServiceRegistry {
    pub fn new_from_cache(config: &Config) -> Result<Self> {
        let mut actions_by_namever: BTreeMap<String, Vec<ActionEntry>> = BTreeMap::new();
        // this is an error if we don't have a cache path
        let cache_path = config
            .get("discovery.cache_path")
            .ok_or(anyhow::anyhow!("No cache path found"))?;

        let mut file = File::open(cache_path).map_err(|e| {
            anyhow::anyhow!("Failed to open discovery cache file {}, {}", cache_path, e)
        })?;

        let iterator = CacheFileAnnouncementIterator::new(&mut file);
        for announcement_packet in iterator {
            let packet = announcement_packet?;
            if !packet.signature_is_valid() {
                continue;
            }

            let AnnouncementPacket { body, .. } = packet;

            for action in &body.actions {
                let entry = ActionEntry {
                    service_info: body.info.clone(),
                    announcement_params: body.params.clone(),
                    action: action.clone(),
                    authorized: true,
                };
                actions_by_namever
                    .entry(action.pathver.clone())
                    .or_default()
                    .push(entry);
            }
        }

        Ok(Self { actions_by_namever })
    }
    pub fn empty() -> Self {
        Self {
            actions_by_namever: BTreeMap::new(),
        }
    }
    // returns an iterator over all actions presently in the registry
    pub fn actions_iter(&self) -> impl Iterator<Item = &ActionEntry> + '_ {
        self.actions_by_namever
            .values()
            .flat_map(|entries| entries.iter())
    }

    pub fn find_actions(&self, pathver: &str) -> Option<Vec<&ActionEntry>> {
        self.actions_by_namever
            .get(pathver)
            .map(|entries| entries.iter().collect())
    }

    pub fn get_action(&self, pathver: &str) -> Option<&ActionEntry> {
        self.actions_by_namever.get(pathver).and_then(|entries| {
            if entries.is_empty() {
                None
            } else {
                let random_index = rand::random::<usize>() % entries.len();
                entries.get(random_index)
            }
        })
    }
}

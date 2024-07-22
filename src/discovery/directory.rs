use std::fs::File;

use announcement::Announcement;
use anyhow::Result;

use super::announcement;

pub struct ServiceList {
    announcements: Vec<Announcement>,
}

impl ServiceList {
    pub fn new(discovery_cache_path: &str) -> Result<Self> {
        let announcements = Vec::new();
        let mut file = File::open(discovery_cache_path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to open discovery cache file {}, {}",
                discovery_cache_path,
                e
            )
        })?;

        Self { announcements }
    }
}

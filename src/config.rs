use anyhow::Result;
use log;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConfElement {
    value: Option<String>,
    list: Vec<ConfElement>,
    children: BTreeMap<String, ConfElement>,
}

pub struct Config {
    root: ConfElement,
}

pub struct ConfigPath {
    path: PathBuf,
    // in certain cases we want to rewrite config parameters with paths
    // based on where we found the config file
    conf_rewrites: Option<Vec<ConfRewrite>>,
}

struct ConfRewrite {
    key_match: Option<Regex>,
    value_match: Option<Regex>,
    value_replacer: Option<String>,
}

static DEFAULT_CONFIG_PATHS: [&str; 2] = ["/etc/scamp/scamp.conf", "/etc/GTSOA/scamp.conf"];

impl Config {
    pub fn new(config_path: Option<String>) -> Result<Self> {
        // get the path of the config file, either from an env variable with dotenv SCAMP_CONFIG
        // or from one of several default paths
        let config_path = Self::get_config_path(config_path)?;

        println!("Using config path: {}", config_path.path.to_str().unwrap());

        let config_contents = std::fs::read_to_string(&config_path.path)?;
        let root = Self::parse_config(&config_contents, config_path.conf_rewrites)?;

        Ok(Self { root })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        match self.root.get(key) {
            Some(ConfElement {
                value: Some(value), ..
            }) => Some(value),
            _ => None,
        }
    }

    fn get_config_path(override_path: Option<String>) -> Result<ConfigPath> {
        dotenv::dotenv().ok();

        if let Some(path) = override_path {
            let path = PathBuf::from(path);
            if path.exists() {
                return Ok(ConfigPath {
                    path,
                    conf_rewrites: None,
                });
            } else {
                return Err(anyhow::anyhow!(
                    "--config path {} does not exist",
                    path.to_string_lossy()
                ));
            }
        }

        if let Ok(config_path) = std::env::var("SCAMP_CONFIG") {
            let path = PathBuf::from(config_path);
            if path.exists() {
                return Ok(ConfigPath {
                    path,
                    conf_rewrites: None,
                });
            } else {
                return Err(anyhow::anyhow!(
                    "SCAMP_CONFIG path {} does not exist",
                    path.to_string_lossy()
                ));
            }
        } else {
            let mut failed_paths = Vec::new();
            // iterate over the default paths and return the first one that exists
            for path in DEFAULT_CONFIG_PATHS {
                let path = PathBuf::from(path);
                if path.exists() {
                    return Ok(ConfigPath {
                        path,
                        conf_rewrites: None,
                    });
                } else {
                    failed_paths.push(path.to_string_lossy().to_string());
                }
            }

            if let Some(home) = homedir::my_home()? {
                let path = home.join("GT/backplane/etc/soa.conf");
                if path.exists() {
                    // rewrite /backplane/*
                    return Ok(ConfigPath {
                        path,
                        conf_rewrites: Some(vec![ConfRewrite {
                            key_match: None,
                            value_match: Some(Regex::new("^/backplane").unwrap()),
                            value_replacer: Some(
                                home.join("GT/backplane").to_string_lossy().to_string(),
                            ),
                        }]),
                    });
                }
                failed_paths.push(path.to_string_lossy().to_string());
            };

            Err(anyhow::anyhow!(
                "No scamp config file found. tried {}",
                failed_paths.join(", ")
            ))
        }
    }

    fn parse_config(config: &str, value_rewrites: Option<Vec<ConfRewrite>>) -> Result<ConfElement> {
        let mut root = ConfElement::default();

        for line in config.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let mut parts = line.splitn(2, '=').map(|s| s.trim());

            let key = parts.next();
            let value = parts.next();

            match (key, value) {
                (Some(key), Some(value)) => {
                    let mut current = &mut root;
                    for segment in key.split('.') {
                        // if segment is numeric, try to convert to u64
                        if let Ok(num) = segment.parse::<usize>() {
                            if current.list.len() <= num {
                                current.list.resize_with(num + 1, ConfElement::default);
                            }
                            current = &mut current.list[num];
                        } else {
                            current = current
                                .children
                                .entry(segment.to_string())
                                .or_insert_with(ConfElement::default);
                        }
                    }
                    let mut value = value.to_string();
                    if let Some(conf_rewrites) = &value_rewrites {
                        for rewrite in conf_rewrites {
                            if let Some(key_match) = &rewrite.key_match {
                                if !key_match.is_match(key) {
                                    continue;
                                }
                            }
                            if let (Some(value_match), Some(value_replacer)) =
                                (&rewrite.value_match, &rewrite.value_replacer)
                            {
                                value =
                                    value_match.replace_all(&value, value_replacer).into_owned();
                            }
                        }
                    }
                    current.value = Some(value);
                }
                _ => {
                    log::warn!("Invalid config line. Skipping: {}", line);
                }
            }
        }

        Ok(root)
    }
}

impl ConfElement {
    pub fn get(&self, key: &str) -> Option<&ConfElement> {
        let mut current = self;
        for segment in key.split('.') {
            current = current.children.get(segment)?;
        }
        Some(current)
    }
    /// writes the config file out to a writable stream in the original format
    /// this is useful for debugging
    #[allow(dead_code)]
    pub fn write_to_file(&self, writable: &mut impl std::io::Write, prefix: &str) -> Result<()> {
        if let Some(value) = &self.value {
            writeln!(writable, "{} = {}", prefix, value)?;
        }

        for (key, child) in &self.children {
            let child_prefix = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };
            child.write_to_file(writable, &child_prefix)?;
        }

        for (i, child) in self.list.iter().enumerate() {
            let child_prefix = format!("{}.{}", prefix, i);
            child.write_to_file(writable, &child_prefix)?;
        }

        Ok(())
    }
    #[allow(dead_code)]
    pub fn to_string(&self) -> String {
        let mut writer = Vec::new();
        self.write_to_file(&mut writer, "").unwrap();
        String::from_utf8_lossy(&writer).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        // The keys in this file are pre-sorted, and it has no comments,
        // so we can parse it, write it back out to a string and it will
        // be the same
        let test_config_file = include_str!("../samples/soa.conf");
        let root = Config::parse_config(test_config_file, None).unwrap();
        // create a writer for a string
        let mut writer = Vec::new();

        root.write_to_file(&mut writer, "").unwrap();
        assert_eq!(test_config_file, String::from_utf8_lossy(&writer));
    }
}

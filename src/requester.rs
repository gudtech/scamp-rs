//! High-level Requester API — Perl Requester.pm:20-43.
//!
//! Combines discovery lookup + connection pooling + request/response
//! into a single async call, matching Perl's simple_request().

use anyhow::{anyhow, Result};

use crate::config::Config;
use crate::discovery::service_registry::ServiceRegistry;
use crate::transport::beepish::proto::EnvelopeFormat;
use crate::transport::beepish::{BeepishClient, ScampResponse};

/// Default per-request (RPC) timeout — Perl ServiceInfo.pm:257
const DEFAULT_RPC_TIMEOUT_SECS: u64 = 75;

/// High-level SCAMP requester: lookup action → connect → send → receive.
pub struct Requester {
    client: BeepishClient,
    registry: ServiceRegistry,
    default_sector: String,
}

impl Requester {
    /// Create a Requester from config, loading the discovery cache.
    pub fn from_config(config: &Config) -> Result<Self> {
        let registry = ServiceRegistry::new_from_cache(config)?;
        let client = BeepishClient::new(config);
        let default_sector = config
            .get::<String>("bus.default_sector")
            .and_then(|r| r.ok())
            .unwrap_or_else(|| "main".to_string());

        Ok(Requester {
            client,
            registry,
            default_sector,
        })
    }

    /// Send a request to a discovered service action.
    /// Perl Requester.pm:20-43 (simple_request).
    pub async fn request(
        &self,
        action: &str,
        version: u32,
        body: Vec<u8>,
    ) -> Result<ScampResponse> {
        self.request_with_opts(RequestOpts {
            action,
            version,
            body,
            sector: &self.default_sector,
            envelope: EnvelopeFormat::Json,
            ticket: "",
            timeout_secs: None,
        })
        .await
    }

    /// Send a request with full control over parameters.
    pub async fn request_with_opts(&self, opts: RequestOpts<'_>) -> Result<ScampResponse> {
        // 1. Lookup action in registry
        let entry = self
            .registry
            .find_action_with_envelope(
                opts.sector,
                opts.action,
                opts.version,
                &envelope_str(&opts.envelope),
            )
            .ok_or_else(|| {
                anyhow!(
                    "Action not found: {}:{}.v{}",
                    opts.sector,
                    opts.action,
                    opts.version
                )
            })?;

        // 2. Resolve timeout: explicit > per-action flag > default
        let timeout_secs = opts
            .timeout_secs
            .or_else(|| entry.timeout_secs())
            .unwrap_or(DEFAULT_RPC_TIMEOUT_SECS);

        // 3. Connect (pooled) and send request
        let resp = self
            .client
            .request(
                &entry.service_info,
                opts.action,
                opts.version as i32,
                opts.envelope.clone(),
                opts.ticket,
                0,
                opts.body,
                Some(timeout_secs),
            )
            .await?;

        // 4. Check for transport-level error
        if let Some(err) = &resp.error {
            return Err(anyhow!("Transport error: {}", err));
        }

        Ok(resp)
    }
}

/// Request parameters for request_with_opts.
pub struct RequestOpts<'a> {
    pub action: &'a str,
    pub version: u32,
    pub body: Vec<u8>,
    pub sector: &'a str,
    pub envelope: EnvelopeFormat,
    pub ticket: &'a str,
    pub timeout_secs: Option<u64>,
}

fn envelope_str(e: &EnvelopeFormat) -> String {
    match e {
        EnvelopeFormat::Json => "json".to_string(),
        EnvelopeFormat::JsonStore => "jsonstore".to_string(),
        EnvelopeFormat::Other(s) => s.clone(),
    }
}

use anyhow::Result;
use scamp::config::Config;
use scamp::service::{ScampReply, ScampService};

#[derive(clap::Parser, Debug, Clone)]
pub struct ServeCommand {
    /// Service name (used in identity and announcements)
    #[arg(short, long, default_value = "scamp-rs-test")]
    name: String,

    /// Sector
    #[arg(short, long, default_value = "main")]
    sector: String,

    /// Path to PEM-encoded service key
    #[arg(long)]
    key: Option<String>,

    /// Path to PEM-encoded service certificate
    #[arg(long)]
    cert: Option<String>,

    /// IP address to announce (overrides auto-detected bind address)
    #[arg(long)]
    announce_ip: Option<String>,
}

impl ServeCommand {
    pub async fn run(&self, config: &Config) -> Result<()> {
        let mut service = ScampService::new(&self.name, &self.sector);

        // Register a simple echo action for testing
        service.register("ScampRsTest.echo", 1, |req| async move {
            println!("  * Received request: action={} body_len={}", req.action, req.body.len());
            ScampReply::ok(req.body)
        });

        // Register a health check
        service.register("ScampRsTest.health_check", 1, |_req| async move {
            ScampReply::ok(b"{}".to_vec())
        });

        // Load TLS key/cert
        let key_path = self.key.clone().unwrap_or_else(|| {
            config
                .get::<String>(&format!("{}.key", self.name))
                .and_then(|r| r.ok())
                .unwrap_or_else(|| {
                    // Fall back to dev key
                    let home = std::env::var("HOME").unwrap_or_default();
                    format!("{}/GT/backplane/devkeys/dev.key", home)
                })
        });

        let cert_path = self.cert.clone().unwrap_or_else(|| {
            config
                .get::<String>(&format!("{}.cert", self.name))
                .and_then(|r| r.ok())
                .unwrap_or_else(|| {
                    let home = std::env::var("HOME").unwrap_or_default();
                    format!("{}/GT/backplane/devkeys/dev.crt", home)
                })
        });

        println!("  * Loading key: {}", key_path);
        println!("  * Loading cert: {}", cert_path);

        let key_pem = std::fs::read(&key_path)?;
        let cert_pem = std::fs::read(&cert_path)?;

        service.bind_pem(&key_pem, &cert_pem).await?;

        // Set announce IP if provided, or auto-detect from hostname
        if let Some(ip) = &self.announce_ip {
            service.set_announce_ip(ip);
        } else {
            // Try to detect a non-loopback IP for announcing
            if let Ok(hostname) = std::env::var("HOSTNAME") {
                // In Docker, HOSTNAME is the container ID; resolve it to get the container IP
                if let Ok(addrs) = tokio::net::lookup_host(format!("{}:0", hostname)).await {
                    for addr in addrs {
                        if !addr.ip().is_loopback() {
                            service.set_announce_ip(&addr.ip().to_string());
                            break;
                        }
                    }
                }
            }
        }

        println!("  * Service identity: {}", service.identity());
        println!("  * Listening on: {}", service.uri().unwrap_or_default());
        println!("  * Registered actions: ScampRsTest.echo~1, ScampRsTest.health_check~1");

        // TODO: implement UDP multicast announcement sending (P4-1)
        // Services announce via multicast; the cache service receives and writes to cache file.
        // Direct cache file writing is wrong — that's exclusively the cache service's job.
        println!("  * WARNING: Multicast announcing not yet implemented. Service will not be discoverable.");
        println!("  * Press Ctrl+C to stop");

        service.run().await
    }
}

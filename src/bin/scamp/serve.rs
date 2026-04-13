use anyhow::Result;
use scamp::bus_info::BusInfo;
use scamp::config::Config;
use scamp::service::{MulticastConfig, ScampReply, ScampService};

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
        service.register("ScampRsTest.health_check", 1, |_req| async move { ScampReply::ok(b"{}".to_vec()) });

        // Load TLS key/cert
        let key_path = self.key.clone().unwrap_or_else(|| {
            config
                .get::<String>(&format!("{}.key", self.name))
                .and_then(|r| r.ok())
                .unwrap_or_else(|| {
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

        // Resolve service address from config — Perl Config.pm:59-112
        let bus_info = BusInfo::from_config(config);
        let bind_ip = bus_info.service_addr();

        service.bind_pem(&key_pem, &cert_pem, bind_ip).await?;

        // Determine announce IP: CLI override > bus_info > hostname detection
        let announce_ip = if let Some(ip) = &self.announce_ip {
            ip.clone()
        } else if !bind_ip.is_unspecified() {
            bind_ip.to_string()
        } else {
            detect_announce_ip().await.unwrap_or_else(|| "127.0.0.1".into())
        };
        service.set_announce_ip(&announce_ip);

        println!("  * Service identity: {}", service.identity());
        println!("  * Listening on: {}", service.uri().unwrap_or_default());
        println!("  * Registered actions: ScampRsTest.echo~1, ScampRsTest.health_check~1");

        // Set up multicast announcing using resolved interface
        let mcast_interface: std::net::Ipv4Addr = announce_ip.parse().unwrap_or(bind_ip);
        let mcast_config = MulticastConfig::from_config(config, mcast_interface);

        println!(
            "  * Multicast: {}:{} every {}s via {}",
            mcast_config.group, mcast_config.port, mcast_config.interval_secs, mcast_config.interface
        );

        // Shutdown signal — cloned for announcer and listener
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let service_shutdown_rx = shutdown_rx.clone();

        // Build packet closure — captures service state for announcer
        let build_packet = {
            let service_ref = &service;
            move |active: bool| -> anyhow::Result<Vec<u8>> { service_ref.build_announcement_packet(active) }
        };

        // Spawn multicast announcer
        let _announcer_handle = {
            // Build initial packet to verify it works before spawning
            let _test_packet = build_packet(true)?;
            println!("  * Announcement packet built successfully");

            // We need to move the build function into the spawned task.
            // Build packets from the service reference directly.
            let identity = service.identity().to_string();
            let sector = self.sector.clone();
            let envelopes = vec!["json".to_string()];
            let uri = service.uri().ok_or_else(|| anyhow::anyhow!("No URI"))?;
            let key = key_pem.clone();
            let cert = cert_pem.clone();
            let actions = service.actions_snapshot();

            tokio::spawn(async move {
                let build_fn = move |active: bool| -> anyhow::Result<Vec<u8>> {
                    scamp::service::announce_raw(&identity, &sector, &envelopes, &uri, &actions, &key, &cert, active)
                };
                if let Err(e) = scamp::service::multicast::run_announcer(mcast_config, build_fn, shutdown_rx).await {
                    log::error!("Announcer failed: {}", e);
                }
            })
        };

        println!("  * Multicast announcing started");
        println!("  * Press Ctrl+C to stop");

        // Handle Ctrl+C for graceful shutdown
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            println!("\n  * Shutting down (sending weight=0 announcements)...");
            let _ = shutdown_tx.send(true);
        });

        // Run the service (accepts connections until shutdown, then drains)
        service.run(service_shutdown_rx).await
    }
}

/// Try to detect a non-loopback IP for announcing (Docker hostname resolution).
async fn detect_announce_ip() -> Option<String> {
    if let Ok(hostname) = std::env::var("HOSTNAME") {
        if let Ok(addrs) = tokio::net::lookup_host(format!("{}:0", hostname)).await {
            for addr in addrs {
                if !addr.ip().is_loopback() {
                    return Some(addr.ip().to_string());
                }
            }
        }
    }
    None
}

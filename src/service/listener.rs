//! TLS listener and SCAMP service setup.
//!
//! Matches Perl Transport::BEEPish::Server.pm.

use anyhow::{anyhow, Result};
use log;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_native_tls::native_tls;
use tokio_native_tls::TlsAcceptor;

use super::announce;
use super::handler::{ActionHandlerFn, ActionInfo, RegisteredAction};
use super::server_connection;

/// SCAMP service that listens for incoming connections and dispatches requests.
pub struct ScampService {
    #[allow(dead_code)] // Used in identity format, will be needed for config
    name: String,
    identity: String,
    sector: String,
    envelopes: Vec<String>,
    actions: HashMap<String, RegisteredAction>,
    listener: Option<TcpListener>,
    tls_acceptor: Option<TlsAcceptor>,
    address: Option<SocketAddr>,
    key_pem: Option<Vec<u8>>,
    cert_pem: Option<Vec<u8>>,
    announce_ip: Option<String>,
}

impl ScampService {
    pub fn new(name: &str, sector: &str) -> Self {
        let random_bytes: [u8; 18] = rand::random();
        let identity_suffix = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, random_bytes);

        ScampService {
            name: name.to_string(),
            identity: format!("{}:{}", name, identity_suffix),
            sector: sector.to_string(),
            envelopes: vec!["json".to_string()],
            actions: HashMap::new(),
            listener: None,
            tls_acceptor: None,
            address: None,
            key_pem: None,
            cert_pem: None,
            announce_ip: None,
        }
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn address(&self) -> Option<SocketAddr> {
        self.address
    }

    pub fn uri(&self) -> Option<String> {
        self.address.map(|addr| {
            let ip = self.announce_ip.as_deref().unwrap_or(&addr.ip().to_string()).to_string();
            format!("beepish+tls://{}:{}", ip, addr.port())
        })
    }

    pub fn set_announce_ip(&mut self, ip: &str) {
        self.announce_ip = Some(ip.to_string());
    }

    /// Snapshot of registered action info for use by the announcer task.
    pub fn actions_snapshot(&self) -> Vec<ActionInfo> {
        self.actions.values().map(ActionInfo::from).collect()
    }

    pub fn register<F, Fut>(&mut self, action: &str, version: i32, handler: F)
    where
        F: Fn(ScampRequest) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ScampReply> + Send + 'static,
    {
        let key = format!("{}.v{}", action.to_lowercase(), version);
        let handler: ActionHandlerFn = Arc::new(move |req| Box::pin(handler(req)));
        self.actions.insert(
            key,
            RegisteredAction {
                name: action.to_string(),
                version,
                flags: vec![],
                handler,
            },
        );
    }

    /// Bind with TLS using specified bind address.
    /// Perl Server.pm:27-34: random port in 30100-30399, bind to service address.
    pub async fn bind_pem(&mut self, key_pem: &[u8], cert_pem: &[u8], bind_ip: std::net::Ipv4Addr) -> Result<()> {
        self.key_pem = Some(key_pem.to_vec());
        self.cert_pem = Some(cert_pem.to_vec());

        let key = native_tls::Identity::from_pkcs8(cert_pem, key_pem)?;
        let tls = native_tls::TlsAcceptor::builder(key).build()?;

        // Perl Server.pm:27-29
        let first_port: u16 = 30100;
        let last_port: u16 = 30399;
        let bind_tries: u32 = 20;

        let mut listener = None;
        for _ in 0..bind_tries {
            let port = first_port + (rand::random::<u16>() % (last_port - first_port + 1));
            let addr = SocketAddr::from((bind_ip, port));
            match TcpListener::bind(addr).await {
                Ok(l) => {
                    listener = Some(l);
                    break;
                }
                Err(_) => continue,
            }
        }

        let listener = listener.ok_or_else(|| anyhow!("Failed to bind after {} tries", bind_tries))?;
        let addr = listener.local_addr()?;
        log::info!("Bound to beepish+tls://{}:{}", addr.ip(), addr.port());

        self.listener = Some(listener);
        self.tls_acceptor = Some(TlsAcceptor::from(tls));
        self.address = Some(addr);
        Ok(())
    }

    /// Build a signed announcement packet (uncompressed bytes).
    /// Perl Announcer.pm:122-204
    pub fn build_announcement_packet(&self, active: bool) -> Result<Vec<u8>> {
        let key_pem = self.key_pem.as_ref().ok_or_else(|| anyhow!("No key"))?;
        let cert_pem = self.cert_pem.as_ref().ok_or_else(|| anyhow!("No cert"))?;
        let uri = self.uri().ok_or_else(|| anyhow!("Not bound"))?;
        let action_infos: Vec<ActionInfo> = self.actions.values().map(ActionInfo::from).collect();

        announce::build_announcement_packet(
            &self.identity,
            &self.sector,
            &self.envelopes,
            &uri,
            &action_infos,
            key_pem,
            cert_pem,
            1, // weight
            5, // interval_secs
            active,
        )
    }

    /// Run the service: accept connections until shutdown signal.
    /// JS service.js:78-91: suspend announcer, drain active requests, then exit.
    pub async fn run(self, mut shutdown_rx: tokio::sync::watch::Receiver<bool>) -> Result<()> {
        let listener = self.listener.ok_or_else(|| anyhow!("Not bound — call bind_pem() first"))?;
        let tls_acceptor = self.tls_acceptor.ok_or_else(|| anyhow!("Not bound — call bind_pem() first"))?;
        let actions = Arc::new(self.actions);
        let active_connections = Arc::new(AtomicU64::new(0));

        // Accept connections until shutdown
        loop {
            tokio::select! {
                result = listener.accept() => {
                    let (stream, peer_addr) = result?;
                    stream.set_nodelay(true)?;
                    let tls_acceptor = tls_acceptor.clone();
                    let actions = actions.clone();
                    let active = active_connections.clone();
                    active.fetch_add(1, Ordering::Relaxed);

                    tokio::spawn(async move {
                        match tls_acceptor.accept(stream).await {
                            Ok(tls_stream) => {
                                log::debug!("Accepted connection from {}", peer_addr);
                                server_connection::handle_connection(tls_stream, actions, None).await;
                            }
                            Err(e) => {
                                log::error!("TLS accept failed from {}: {}", peer_addr, e);
                            }
                        }
                        active.fetch_sub(1, Ordering::Relaxed);
                    });
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
            }
        }

        // Drain active connections (30s timeout)
        let active = active_connections.load(Ordering::Relaxed);
        if active > 0 {
            log::info!("Draining {} active connection(s)...", active);
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
            while active_connections.load(Ordering::Relaxed) > 0 && tokio::time::Instant::now() < deadline {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            let remaining = active_connections.load(Ordering::Relaxed);
            if remaining > 0 {
                log::warn!("Shutdown timeout: {} connections still active", remaining);
            }
        }

        Ok(())
    }
}

// Re-export for use by register() callers
use super::handler::{ScampReply, ScampRequest};

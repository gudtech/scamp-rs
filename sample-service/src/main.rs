//! Sample SCAMP service demonstrating the #[rpc] macro and auto-discovery.
//!
//! Run with: cargo run -p sample-service
//!
//! The service registers actions across multiple namespaces and sectors.
//! Action handlers live in src/actions/, one file per namespace leaf.
//! The module path determines the SCAMP namespace automatically.

use std::net::Ipv4Addr;
use std::sync::Arc;

use anyhow::Result;
use scamp::rpc_support::auto_discover_into;
use scamp::service::ScampService;

// Action modules — each file registers its handlers via #[scamp::rpc]
mod actions;

/// Shared state available to all action handlers as `&AppState`.
pub struct AppState {
    pub service_name: String,
    pub start_time: std::time::Instant,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let state = Arc::new(AppState {
        service_name: "SampleService".into(),
        start_time: std::time::Instant::now(),
    });

    let mut service = ScampService::new("SampleService", "main");
    auto_discover_into(&mut service, state, "main");

    // Self-signed cert for demo (production would use real certs from config)
    let (key_pem, cert_pem) = generate_demo_keypair();
    service.bind_pem(&key_pem, &cert_pem, Ipv4Addr::LOCALHOST).await?;

    log::info!("Service: {} ({})", service.identity(), service.address().unwrap());
    for action in service.actions_snapshot() {
        log::info!("  {}.v{} [{}]", action.name, action.version, action.flags.join(","));
    }

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        log::info!("Shutting down...");
        shutdown_tx.send(true).ok();
    });

    service.run(shutdown_rx).await
}

fn generate_demo_keypair() -> (Vec<u8>, Vec<u8>) {
    use openssl::asn1::Asn1Time;
    use openssl::hash::MessageDigest;
    use openssl::pkey::PKey;
    use openssl::rsa::Rsa;
    use openssl::x509::{X509Builder, X509NameBuilder};

    let rsa = Rsa::generate(2048).unwrap();
    let pkey = PKey::from_rsa(rsa).unwrap();
    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_text("CN", "scamp-sample").unwrap();
    let name = name.build();
    let mut builder = X509Builder::new().unwrap();
    builder.set_version(2).unwrap();
    builder.set_subject_name(&name).unwrap();
    builder.set_issuer_name(&name).unwrap();
    builder.set_pubkey(&pkey).unwrap();
    builder.set_not_before(&Asn1Time::days_from_now(0).unwrap()).unwrap();
    builder.set_not_after(&Asn1Time::days_from_now(365).unwrap()).unwrap();
    builder.sign(&pkey, MessageDigest::sha256()).unwrap();
    (pkey.private_key_to_pem_pkcs8().unwrap(), builder.build().to_pem().unwrap())
}

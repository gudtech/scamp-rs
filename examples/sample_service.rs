//! Sample SCAMP service demonstrating the #[rpc] macro and auto-discovery.
//!
//! Run with: cargo run --example sample_service
//!
//! This service registers actions across multiple namespaces and sectors,
//! using various return types and flags. It binds to localhost with a
//! self-signed cert (for demonstration only).

use std::net::Ipv4Addr;
use std::sync::Arc;

use anyhow::Result;
use scamp::rpc_support::{auto_discover_into, Json, RequestContext};
use scamp::service::ScampService;

// ---------------------------------------------------------------------------
// Shared service state — available to all handlers via &AppState
// ---------------------------------------------------------------------------

struct AppState {
    service_name: String,
    start_time: std::time::Instant,
}

// ---------------------------------------------------------------------------
// API.Status namespace — health checks, service info (noauth)
// ---------------------------------------------------------------------------

#[scamp::rpc(noauth, namespace = "API.Status")]
async fn health_check(_ctx: RequestContext, _state: &AppState) -> &'static str {
    "ok"
}

#[scamp::rpc(noauth, namespace = "API.Status")]
async fn info(_ctx: RequestContext, state: &AppState) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": state.service_name,
        "uptime_secs": state.start_time.elapsed().as_secs(),
    }))
}

// ---------------------------------------------------------------------------
// Echo namespace — for testing (echo back the request body)
// ---------------------------------------------------------------------------

#[scamp::rpc(noauth, namespace = "ScampRsTest")]
async fn echo(ctx: RequestContext, _state: &AppState) -> Vec<u8> {
    ctx.body
}

// ---------------------------------------------------------------------------
// Constant.Ship.Carrier namespace — read-only data lookups
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct Carrier {
    id: u64,
    name: String,
    code: String,
}

#[scamp::rpc(read, namespace = "Constant.Ship.Carrier")]
async fn fetch(_ctx: RequestContext, _state: &AppState) -> Json<Vec<Carrier>> {
    // In production, this would query a database
    Json(vec![
        Carrier {
            id: 1,
            name: "UPS".into(),
            code: "UPS".into(),
        },
        Carrier {
            id: 2,
            name: "FedEx".into(),
            code: "FEDEX".into(),
        },
        Carrier {
            id: 3,
            name: "USPS".into(),
            code: "USPS".into(),
        },
    ])
}

// ---------------------------------------------------------------------------
// Order.Shipment namespace — business logic with error handling
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct TrackRequest {
    tracking_number: String,
}

#[derive(serde::Serialize)]
struct TrackResponse {
    tracking_number: String,
    status: String,
    location: String,
}

#[scamp::rpc(version = 2, timeout = 30, namespace = "Order.Shipment")]
async fn track(ctx: RequestContext, _state: &AppState) -> Result<Json<TrackResponse>> {
    let req: TrackRequest = ctx.json()?;

    if req.tracking_number.is_empty() {
        anyhow::bail!("tracking_number is required");
    }

    // In production, this would call a carrier API
    Ok(Json(TrackResponse {
        tracking_number: req.tracking_number,
        status: "in_transit".into(),
        location: "Memphis, TN".into(),
    }))
}

// ---------------------------------------------------------------------------
// Background.Worker namespace — on a different sector
// ---------------------------------------------------------------------------

#[scamp::rpc(noauth, sector = "background", namespace = "Background.Worker")]
async fn process(ctx: RequestContext, _state: &AppState) -> Result<Json<serde_json::Value>> {
    let job: serde_json::Value = ctx.json()?;
    log::info!("Processing background job: {}", job);
    Ok(Json(serde_json::json!({ "status": "completed", "job": job })))
}

// ---------------------------------------------------------------------------
// Service main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // 1. Create shared state
    let state = Arc::new(AppState {
        service_name: "SampleService".into(),
        start_time: std::time::Instant::now(),
    });

    // 2. Create service and auto-discover all #[rpc] handlers
    let mut service = ScampService::new("SampleService", "main");
    auto_discover_into(&mut service, state, "main");

    // 3. Generate a self-signed cert for demo purposes
    let (key_pem, cert_pem) = generate_demo_keypair();
    service.bind_pem(&key_pem, &cert_pem, Ipv4Addr::LOCALHOST).await?;

    log::info!("Service identity: {}", service.identity());
    log::info!("Listening on: {:?}", service.address());
    log::info!("Registered actions:");
    for action in service.actions_snapshot() {
        log::info!("  {}.v{} [{}]", action.name, action.version, action.flags.join(","));
    }

    // 4. Run until Ctrl+C
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

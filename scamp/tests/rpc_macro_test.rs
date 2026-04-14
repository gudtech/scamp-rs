//! Test the #[rpc] macro and auto-discovery.

use scamp::rpc_support::{auto_discover_into, Json, RequestContext};
use scamp::service::{ScampReply, ScampService};
use std::sync::Arc;

// -- Shared service state --
struct AppState {
    greeting: String,
}

// -- Handlers with various return types (Axum-style flexibility) --

// Returns ScampReply directly (explicit control)
#[scamp::rpc(noauth, namespace = "API.Status")]
async fn health_check(_ctx: RequestContext, _state: &AppState) -> ScampReply {
    ScampReply::ok(b"ok".to_vec())
}

// Returns a String (auto-converted to reply body)
#[scamp::rpc(namespace = "API.Status")]
async fn version(_ctx: RequestContext, state: &AppState) -> String {
    state.greeting.clone()
}

// Returns Vec<u8> (raw bytes)
#[scamp::rpc(read, namespace = "Constant.Ship.Carrier")]
async fn fetch(ctx: RequestContext, _state: &AppState) -> Vec<u8> {
    ctx.body
}

// Returns Result — errors auto-convert to error replies
#[scamp::rpc(version = 2, timeout = 600, namespace = "Order.Shipment")]
async fn track(ctx: RequestContext, _state: &AppState) -> anyhow::Result<Vec<u8>> {
    if ctx.body.is_empty() {
        anyhow::bail!("tracking number required");
    }
    Ok(ctx.body)
}

// Returns Json<T> — auto-serialized
#[scamp::rpc(noauth, namespace = "API.Status")]
async fn info(_ctx: RequestContext, state: &AppState) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "greeting": state.greeting }))
}

// Returns &str (static)
#[scamp::rpc(noauth, sector = "background", namespace = "Background.Worker")]
async fn process(_ctx: RequestContext, _state: &AppState) -> &'static str {
    "processed"
}

#[test]
fn test_auto_discover_registers_actions() {
    let state = Arc::new(AppState {
        greeting: "hello from scamp-rs".to_string(),
    });

    let mut service = ScampService::new("TestService", "main");
    auto_discover_into(&mut service, state, "main");

    // Verify actions were registered by checking the snapshot
    let actions = service.actions_snapshot();
    let names: Vec<String> = actions.iter().map(|a| format!("{}.v{}", a.name, a.version)).collect();

    assert!(
        names.contains(&"API.Status.healthCheck.v1".to_string()),
        "missing healthCheck: {:?}",
        names
    );
    assert!(names.contains(&"API.Status.version.v1".to_string()), "missing version: {:?}", names);
    assert!(names.contains(&"API.Status.info.v1".to_string()), "missing info: {:?}", names);
    assert!(
        names.contains(&"Constant.Ship.Carrier.fetch.v1".to_string()),
        "missing fetch: {:?}",
        names
    );
    assert!(
        names.contains(&"Order.Shipment.track.v2".to_string()),
        "missing track v2: {:?}",
        names
    );
    assert!(
        names.contains(&"Background.Worker.process.v1".to_string()),
        "missing process: {:?}",
        names
    );

    // Check flags
    let health = actions.iter().find(|a| a.name == "API.Status.healthCheck").unwrap();
    assert!(health.flags.contains(&"noauth".to_string()), "healthCheck should be noauth");

    let fetch_action = actions.iter().find(|a| a.name == "Constant.Ship.Carrier.fetch").unwrap();
    assert!(fetch_action.flags.contains(&"read".to_string()), "fetch should have read flag");

    let track_action = actions.iter().find(|a| a.name == "Order.Shipment.track").unwrap();
    assert_eq!(track_action.version, 2);
    assert!(track_action.flags.contains(&"t600".to_string()), "track should have t600 flag");
}

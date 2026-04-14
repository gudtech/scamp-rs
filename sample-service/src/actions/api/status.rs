use scamp::rpc_support::{Json, RequestContext};

use crate::AppState;

#[scamp::rpc(noauth)]
pub async fn health_check(_ctx: RequestContext, _state: &AppState) -> &'static str {
    "ok"
}

#[scamp::rpc(noauth)]
pub async fn info(_ctx: RequestContext, state: &AppState) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": state.service_name,
        "uptime_secs": state.start_time.elapsed().as_secs(),
    }))
}

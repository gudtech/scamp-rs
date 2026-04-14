use anyhow::Result;
use scamp::rpc_support::{Json, RequestContext};

use crate::AppState;

#[scamp::rpc(noauth, sector = "background")]
pub async fn process(ctx: RequestContext, _state: &AppState) -> Result<Json<serde_json::Value>> {
    let job: serde_json::Value = ctx.json()?;
    log::info!("Processing background job: {}", job);
    Ok(Json(serde_json::json!({ "status": "completed", "job": job })))
}

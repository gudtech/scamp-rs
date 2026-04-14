use anyhow::Result;
use scamp::rpc_support::{Json, RequestContext};

use crate::AppState;

#[derive(serde::Deserialize)]
struct TrackRequest {
    tracking_number: String,
}

#[derive(serde::Serialize)]
pub(crate) struct TrackResponse {
    tracking_number: String,
    status: String,
    location: String,
}

#[scamp::rpc(version = 2, timeout = 30)]
pub async fn track(ctx: RequestContext, _state: &AppState) -> Result<Json<TrackResponse>> {
    let req: TrackRequest = ctx.json()?;

    if req.tracking_number.is_empty() {
        anyhow::bail!("tracking_number is required");
    }

    // In production this would call a carrier API
    Ok(Json(TrackResponse {
        tracking_number: req.tracking_number,
        status: "in_transit".into(),
        location: "Memphis, TN".into(),
    }))
}

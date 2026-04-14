use scamp::rpc_support::{Json, RequestContext};

use crate::AppState;

#[derive(serde::Serialize)]
pub struct Carrier {
    pub id: u64,
    pub name: String,
    pub code: String,
}

#[scamp::rpc(read)]
pub async fn fetch(_ctx: RequestContext, _state: &AppState) -> Json<Vec<Carrier>> {
    // In production this would query a database
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

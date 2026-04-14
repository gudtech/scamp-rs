use scamp::rpc_support::RequestContext;

use crate::AppState;

#[scamp::rpc(noauth)]
pub async fn echo(ctx: RequestContext, _state: &AppState) -> Vec<u8> {
    ctx.body
}

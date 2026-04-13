//! Service-side types: request, reply, handler function signature.

use std::sync::Arc;

use crate::transport::beepish::proto::{EnvelopeFormat, FlexInt};

/// A request received by the service.
pub struct ScampRequest {
    pub action: String,
    pub version: i32,
    pub envelope: EnvelopeFormat,
    pub request_id: FlexInt,
    pub client_id: FlexInt,
    pub ticket: String,
    pub identifying_token: String,
    pub body: Vec<u8>,
}

/// A response to send back.
pub struct ScampReply {
    pub body: Vec<u8>,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

impl ScampReply {
    pub fn ok(body: Vec<u8>) -> Self {
        ScampReply {
            body,
            error: None,
            error_code: None,
        }
    }

    pub fn error(message: String, code: String) -> Self {
        ScampReply {
            body: vec![],
            error: Some(message),
            error_code: Some(code),
        }
    }
}

/// Handler function type for registered actions.
pub type ActionHandlerFn =
    Arc<dyn Fn(ScampRequest) -> std::pin::Pin<Box<dyn std::future::Future<Output = ScampReply> + Send>> + Send + Sync>;

/// A registered action with its handler.
pub(crate) struct RegisteredAction {
    pub name: String,
    pub version: i32,
    pub flags: Vec<String>,
    pub handler: ActionHandlerFn,
}

/// Lightweight action info for announcement building (no handler).
#[derive(Clone, Debug)]
pub struct ActionInfo {
    pub name: String,
    pub version: i32,
    pub flags: Vec<String>,
}

impl From<&RegisteredAction> for ActionInfo {
    fn from(ra: &RegisteredAction) -> Self {
        ActionInfo {
            name: ra.name.clone(),
            version: ra.version,
            flags: ra.flags.clone(),
        }
    }
}

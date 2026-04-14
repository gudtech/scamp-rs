//! Support types for the `#[rpc]` macro and auto-discovery.
//!
//! This module provides the infrastructure that connects `#[rpc]`-annotated
//! handler functions to `ScampService` via `inventory`.

use std::any::Any;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::service::handler::ScampReply;

// ---------------------------------------------------------------------------
// IntoScampReply — Axum-style flexible return types for handlers
// ---------------------------------------------------------------------------

/// Trait for types that can be converted into a ScampReply.
/// Implement this for custom return types. The `#[rpc]` macro calls
/// `.into_scamp_reply()` on whatever the handler returns.
pub trait IntoScampReply {
    fn into_scamp_reply(self) -> ScampReply;
}

impl IntoScampReply for ScampReply {
    fn into_scamp_reply(self) -> ScampReply {
        self
    }
}

impl IntoScampReply for Vec<u8> {
    fn into_scamp_reply(self) -> ScampReply {
        ScampReply::ok(self)
    }
}

impl IntoScampReply for String {
    fn into_scamp_reply(self) -> ScampReply {
        ScampReply::ok(self.into_bytes())
    }
}

impl IntoScampReply for &str {
    fn into_scamp_reply(self) -> ScampReply {
        ScampReply::ok(self.as_bytes().to_vec())
    }
}

impl<T: IntoScampReply, E: std::fmt::Display> IntoScampReply for Result<T, E> {
    fn into_scamp_reply(self) -> ScampReply {
        match self {
            Ok(v) => v.into_scamp_reply(),
            Err(e) => ScampReply::error(e.to_string(), "error".to_string()),
        }
    }
}

/// Wrapper for returning JSON-serialized responses from handlers.
/// Usage: `return Json(my_struct)` or `Ok(Json(my_struct))`
pub struct Json<T: serde::Serialize>(pub T);

impl<T: serde::Serialize> IntoScampReply for Json<T> {
    fn into_scamp_reply(self) -> ScampReply {
        match serde_json::to_vec(&self.0) {
            Ok(body) => ScampReply::ok(body),
            Err(e) => ScampReply::error(e.to_string(), "serialization_error".to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// RequestContext
// ---------------------------------------------------------------------------

/// A request context passed to `#[rpc]` handlers.
pub struct RequestContext {
    pub action: String,
    pub version: i32,
    pub client_id: i64,
    pub ticket: String,
    pub identifying_token: String,
    pub body: Vec<u8>,
}

impl RequestContext {
    /// Deserialize the request body as JSON.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }
}

/// Type alias for the boxed future returned by handlers.
pub type HandlerFuture = Pin<Box<dyn Future<Output = ScampReply> + Send>>;

/// Type-erased handler: takes context + owned state Arc, returns boxed future.
/// State is Arc'd so the future can be 'static (no borrowed data).
pub type DynHandler = Box<dyn Fn(RequestContext, Arc<dyn Any + Send + Sync>) -> HandlerFuture + Send + Sync>;

/// Registration entry emitted by the `#[rpc]` macro via `inventory::submit!`.
pub struct RpcRegistration {
    /// Returns the namespace (from module_path!() or explicit override).
    pub namespace_fn: fn() -> String,
    /// Wire name of the action method (camelCase).
    pub wire_name: &'static str,
    /// Action version (default 1).
    pub version: u32,
    /// Flags: "noauth", "read", "public", "t600", etc.
    pub flags: &'static [&'static str],
    /// Optional sector override (None = use service default).
    pub sector_fn: fn() -> Option<String>,
    /// The handler — type-erased. The macro wraps the real handler in a closure
    /// that downcasts `&dyn Any` to `&S`.
    pub make_handler: fn() -> DynHandler,
}

inventory::collect!(RpcRegistration);

/// Iterate all `#[rpc]`-registered actions.
pub fn discover_actions() -> inventory::iter<RpcRegistration> {
    inventory::iter::<RpcRegistration>
}

/// Full action path for a registration.
pub fn action_path(reg: &RpcRegistration) -> String {
    let ns = (reg.namespace_fn)();
    if ns.is_empty() {
        reg.wire_name.to_string()
    } else {
        format!("{}.{}", ns, reg.wire_name)
    }
}

/// Convert a Rust module path to a SCAMP namespace.
///
/// `my_crate::actions::config::user::credentials` → `Config.User.Credentials`
///
/// Strips everything up to and including the `actions::` segment,
/// then converts each remaining segment from snake_case to PascalCase.
pub fn module_path_to_namespace(module_path: &str) -> String {
    let segments: Vec<&str> = module_path.split("::").collect();
    let start = segments.iter().position(|&s| s == "actions").map(|i| i + 1).unwrap_or(1); // fallback: skip crate name
    segments[start..].iter().map(|s| snake_to_pascal(s)).collect::<Vec<_>>().join(".")
}

fn snake_to_pascal(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for ch in s.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Helper to create a type-erased handler from a concrete async fn.
/// Used by the `#[rpc]` macro expansion. `S` is the service state type.
/// The handler closure wraps the async fn with `Box::pin` to erase the lifetime.
pub fn make_handler_erased<S, F>(handler: F) -> DynHandler
where
    S: Send + Sync + 'static,
    F: for<'a> Fn(RequestContext, &'a S) -> Pin<Box<dyn Future<Output = ScampReply> + Send + 'a>> + Send + Sync + 'static,
{
    let handler = Arc::new(handler);
    Box::new(move |ctx, state: Arc<dyn Any + Send + Sync>| -> HandlerFuture {
        let state = state.downcast::<S>().expect("RPC handler state type mismatch");
        let handler = handler.clone();
        Box::pin(async move { handler(ctx, &state).await })
    })
}

/// Register all discovered `#[rpc]` actions into a ScampService.
/// `S` is the service state type — must match what handlers expect.
pub fn auto_discover_into<S: Send + Sync + 'static>(service: &mut crate::service::ScampService, state: Arc<S>, default_sector: &str) {
    let state_any: Arc<dyn Any + Send + Sync> = state;

    for reg in inventory::iter::<RpcRegistration> {
        let path = action_path(reg);
        let sector = (reg.sector_fn)().unwrap_or_else(|| default_sector.to_string());
        let flags: Vec<String> = reg.flags.iter().map(|s| s.to_string()).collect();
        let version = reg.version as i32;
        let handler: Arc<DynHandler> = Arc::new((reg.make_handler)());
        let state_ref = state_any.clone();

        let action_handler = move |req: crate::service::ScampRequest| {
            let ctx = RequestContext {
                action: req.action,
                version: req.version,
                client_id: req.client_id.0,
                ticket: req.ticket,
                identifying_token: req.identifying_token,
                body: req.body,
            };
            let handler = handler.clone();
            let state = state_ref.clone();
            async move { handler(ctx, state).await }
        };

        let flags_refs: Vec<&str> = flags.iter().map(|s| s.as_str()).collect();
        service.register_with_flags(&path, version, &flags_refs, action_handler);
        log::info!("Registered action: {}.v{} [{}] sector={}", path, version, flags.join(","), sector);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_path_to_namespace() {
        assert_eq!(
            module_path_to_namespace("my_crate::actions::config::user::credentials"),
            "Config.User.Credentials"
        );
        assert_eq!(
            module_path_to_namespace("scamp::actions::constant::ship::carrier"),
            "Constant.Ship.Carrier"
        );
        assert_eq!(module_path_to_namespace("scamp::actions::download::po"), "Download.Po");
        assert_eq!(
            module_path_to_namespace("scamp::actions::product::photo_sample"),
            "Product.PhotoSample"
        );
    }

    #[test]
    fn test_module_path_no_actions_segment() {
        assert_eq!(module_path_to_namespace("scamp::handlers::echo"), "Handlers.Echo");
    }

    #[test]
    fn test_snake_to_pascal() {
        assert_eq!(snake_to_pascal("carrier_class"), "CarrierClass");
        assert_eq!(snake_to_pascal("config"), "Config");
        assert_eq!(snake_to_pascal("photo_sample"), "PhotoSample");
    }
}

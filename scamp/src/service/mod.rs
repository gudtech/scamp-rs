//! SCAMP service: accepts incoming connections, dispatches requests to handlers.
//!
//! Implements the server side of the SCAMP protocol, matching
//! Perl Transport::BEEPish::Server and JS actor/service.js.

mod announce;
pub(crate) mod handler;
mod listener;
pub mod multicast;
pub(crate) mod server_connection;
#[cfg(test)]
mod server_connection_tests;
mod server_reply;

pub use handler::{ActionHandlerFn, ActionInfo, ScampReply, ScampRequest};
pub use listener::ScampService;
pub use multicast::MulticastConfig;

/// Build a raw announcement packet from action info (for use by announcer task).
pub fn announce_raw(
    identity: &str,
    sector: &str,
    envelopes: &[String],
    uri: &str,
    actions: &[ActionInfo],
    key_pem: &[u8],
    cert_pem: &[u8],
    active: bool,
) -> anyhow::Result<Vec<u8>> {
    announce::build_announcement_packet(identity, sector, envelopes, uri, actions, key_pem, cert_pem, 1, 5, active)
}

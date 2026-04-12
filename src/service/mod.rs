//! SCAMP service: accepts incoming connections, dispatches requests to handlers.
//!
//! Implements the server side of the SCAMP protocol, matching
//! Perl Transport::BEEPish::Server and JS actor/service.js.

mod announce;
mod handler;
mod listener;

pub use handler::{ActionHandlerFn, ScampReply, ScampRequest};
pub use listener::ScampService;

//! BEEPish: SCAMP's multiplexed connection protocol.
//! Based on RFC 3080/3081, providing concurrent request/response
//! over a single TCP connection without head-of-line blocking.
mod client;
pub mod proto;

pub use client::{BeepishClient, ConnectionHandle, ScampResponse};

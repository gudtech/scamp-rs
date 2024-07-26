mod client;
///! Beepish is a protocol based heavily on rfc3080 / rfc3081
///! The purpose of which is to provide for managing multiple concurrent
///! Requests over a single tcp connection without head-of-line blocking
///! More information can be found at https://www.beepcore.org/
mod proto;

pub use client::BeepishClient;

// The nightly features that are commonly needed with async / await
#![feature(await_macro, async_await, futures_api)]
//
//// enable the await! macro, async support, and the new std::Futures api.
//#![feature(await_macro, async_await, futures_api)]
//
//// only needed if we want to manually write a method to go forward from 0.1 to 0.3 future,
//// or manually implement a std future (it provides Pin and Unpin):
//#![feature(pin)]
//// only needed to manually implement a std future:
//#![feature(arbitrary_self_types)]


// This pulls in the `tokio-async-await` crate. While Rust 2018 doesn't require
// `extern crate`, we need to pull in the macros.
#[macro_use]
extern crate tokio;

mod packet;
pub mod message;
pub mod agent;
pub mod transport;
pub mod error;

pub use crate::message::Message;
pub use crate::agent::Agent;
pub use crate::error::Error;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}

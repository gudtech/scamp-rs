
//! Streaming bodies for Requests and Responses
//!
//! For both [Clients](::client) and [Servers](::server), requests and
//! responses use streaming bodies, instead of complete buffering. This
//! allows applications to not use memory they don't need, and allows exerting
//! back-pressure on connections by only reading when asked.
//!
//! There are two pieces to this in scamp:
//!
//! - The [`Payload`](body::Payload) trait the describes all possible bodies. scamp
//!   allows any body type that implements `Payload`, allowing applications to
//!   have fine-grained control over their streaming.
//! - The [`Body`](Body) concrete type, which is an implementation of `Payload`,
//!  and returned by scamp as a "receive stream" (so, for server requests and
//!  client responses). It is also a decent default implementation if you don't
//!  have very custom needs of your send streams.

pub use self::message::{Message, Sender};
pub use self::packet::Packet;
pub use self::payload::Payload;
//
mod message;
mod packet;
mod payload;
//
//// The full_data API is not stable, so these types are to try to prevent
//// users from being able to:
////
//// - Implment `__scamp_full_data` on their own Payloads.
//// - Call `__scamp_full_data` on any Payload.
////
//// That's because to implement it, they need to name these types, and
//// they can't because they aren't exported. And to call it, they would
//// need to create one of these values, which they also can't.

pub(crate) mod internal {
    #[allow(missing_debug_implementations)]
    pub struct FullDataArg(pub(crate) ());
    #[allow(missing_debug_implementations)]
    pub struct FullDataRet<B>(pub(crate) Option<B>);
}

fn _assert_send_sync() {
    fn _assert_send<T: Send>() {}
    fn _assert_sync<T: Sync>() {}

    _assert_send::<Message>();
    _assert_send::<Packet>();
    _assert_sync::<Packet>();
}
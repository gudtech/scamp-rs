use std::borrow::Cow;
use std::fmt;
use std::task::Poll;

use bytes::Bytes;
use futures::Stream;
use http::HeaderMap;
use tokio::sync::{mpsc, oneshot};

use super::internal::{FullDataArg, FullDataRet};
use super::{Packet, Payload};
use crate::common::Never;
use crate::Error;
//use upgrade::OnUpgrade;

type MessageSender = mpsc::Sender<Result<Packet, Error>>;

/// A stream of `Packet`s, used when receiving bodies.
///
/// A good default `Payload` to use in many applications.
///
/// Also implements `futures::Stream`, so stream combinators may be used.
#[must_use = "streams do nothing unless polled"]
pub struct Message {
    //    kind: Kind,
}

enum Kind {
    Once(Option<Packet>),
    Chan {
        content_length: Option<u64>,
        abort_rx: oneshot::Receiver<()>,
        rx: mpsc::Receiver<Result<Packet, Error>>,
    },
    //    H2 {
    //        content_length: Option<u64>,
    //        recv: h2::RecvStream,
    //    },
    Wrapped(
        Box<dyn Stream<Item = Result<Packet, Box<dyn ::std::error::Error + Send + Sync>>> + Send>,
    ),
}
//
//type DelayEofUntil = oneshot::Receiver<Never>;
//
//enum DelayEof {
//    /// Initial state, stream hasn't seen EOF yet.
//    NotEof(DelayEofUntil),
//    /// Transitions to this state once we've seen `poll` try to
//    /// return EOF (`None`). This future is then polled, and
//    /// when it completes, the Message finally returns EOF (`None`).
//    Eof(DelayEofUntil),
//}
//
///// A sender half used with `Message::channel()`.
/////
///// Useful when wanting to stream chunks from another thread. See
///// [`Message::channel`](Message::channel) for more.
#[must_use = "Sender does nothing unless sent on"]
#[derive(Debug)]
pub struct Sender {
    abort_tx: oneshot::Sender<()>,
    tx: MessageSender,
}

impl Message {
    /// Create an empty `Message` stream.
    ///
    /// # Example
    ///
    /// ```
    /// use scamp::Message;
    ///
    /// // create a `GET /` request
    /// let get = Message::empty();
    /// ```
    #[inline]
    pub fn empty() -> Message {
        Message::new(Kind::Once(None))
    }

    //    /// Create a `Message` stream with an associated sender half.
    //    ///
    //    /// Useful when wanting to stream chunks from another thread.
    //    #[inline]
    //    pub fn channel() -> (Sender, Message) {
    //        Self::new_channel(None)
    //    }
    //
    //    pub(crate) fn new_channel(content_length: Option<u64>) -> (Sender, Message) {
    //        let (tx, rx) = mpsc::channel(0);
    //        let (abort_tx, abort_rx) = oneshot::channel();
    //
    //        let tx = Sender {
    //            abort_tx: abort_tx,
    //            tx: tx,
    //        };
    //        let rx = Message::new(Kind::Chan {
    //            content_length,
    //            abort_rx,
    //            rx,
    //        });
    //
    //        (tx, rx)
    //    }

    //    /// Wrap a futures `Stream` in a box inside `Message`.
    //    ///
    //    /// # Example
    //    ///
    //    /// ```
    //    /// # extern crate futures;
    //    /// # extern crate scamp;
    //    /// # use scamp::Message;
    //    /// # fn main() {
    //    /// let chunks = vec![
    //    ///     "hello",
    //    ///     " ",
    //    ///     "world",
    //    /// ];
    //    ///
    //    /// let stream = futures::stream::iter_ok::<_, ::std::io::Error>(chunks);
    //    ///
    //    /// let body = Message::wrap_stream(stream);
    //    /// # }
    //    /// ```
    //    pub fn wrap_stream<S>(stream: S) -> Message
    //        where
    //            S: Stream + Send + 'static,
    //            S::Error: Into<Box<::std::error::Error + Send + Sync>>,
    //            Packet: From<S::Item>,
    //    {
    //        let mapped = stream.map(Packet::from).map_err(Into::into);
    //        Message::new(Kind::Wrapped(Box::new(mapped)))
    //    }

    fn new(kind: Kind) -> Message {
        Message {
//            kind: kind,
        }
    }

    //    pub(crate) fn h2(recv: h2::RecvStream, content_length: Option<u64>) -> Self {
    //        Message::new(Kind::H2 {
    //            content_length,
    //            recv,
    //        })
    //    }

    //    pub(crate) fn delayed_eof(&mut self, fut: DelayEofUntil) {
    //        self.extra_mut().delayed_eof = Some(DelayEof::NotEof(fut));
    //    }

    //    fn take_delayed_eof(&mut self) -> Option<DelayEof> {
    //        self
    //            .extra
    //            .as_mut()
    //            .and_then(|extra| extra.delayed_eof.take())
    //    }
    //
    //    fn poll_eof(&mut self) -> Poll<Option<Packet>, ::Error> {
    //        unimplemented!()
    //
    //        match self.take_delayed_eof() {
    //            Some(DelayEof::NotEof(mut delay)) => {
    //                match self.poll_inner() {
    //                    ok @ Ok(Async::Ready(Some(..))) |
    //                    ok @ Ok(Async::NotReady) => {
    //                        self.extra_mut().delayed_eof = Some(DelayEof::NotEof(delay));
    //                        ok
    //                    },
    //                    Ok(Async::Ready(None)) => match delay.poll() {
    //                        Ok(Async::Ready(never)) => match never {},
    //                        Ok(Async::NotReady) => {
    //                            self.extra_mut().delayed_eof = Some(DelayEof::Eof(delay));
    //                            Ok(Async::NotReady)
    //                        },
    //                        Err(_done) => {
    //                            Ok(Async::Ready(None))
    //                        },
    //                    },
    //                    Err(e) => Err(e),
    //                }
    //            },
    //            Some(DelayEof::Eof(mut delay)) => {
    //                match delay.poll() {
    //                    Ok(Async::Ready(never)) => match never {},
    //                    Ok(Async::NotReady) => {
    //                        self.extra_mut().delayed_eof = Some(DelayEof::Eof(delay));
    //                        Ok(Async::NotReady)
    //                    },
    //                    Err(_done) => {
    //                        Ok(Async::Ready(None))
    //                    },
    //                }
    //            },
    //            None => self.poll_inner(),
    ////        }
    //    }
    //
    //    fn poll_inner(&mut self) -> Poll<Option<Packet>, Error> {
    //        match self.kind {
    //            Kind::Once(ref mut val) => Ok(Async::Ready(val.take())),
    //            Kind::Chan {
    //                content_length: ref mut len,
    //                ref mut rx,
    //                ref mut abort_rx,
    //            } => {
    //                if let Ok(Async::Ready(())) = abort_rx.poll() {
    //                    unimplemented!();
    ////                    return Err(::Error::new_body_write("body write aborted"));
    //                }
    //
    //                match rx.poll().expect("mpsc cannot error") {
    //                    Async::Ready(Some(Ok(chunk))) => {
    //                        if let Some(ref mut len) = *len {
    //                            debug_assert!(*len >= chunk.len() as u64);
    //                            *len = *len - chunk.len() as u64;
    //                        }
    //                        Ok(Async::Ready(Some(chunk)))
    //                    }
    //                    Async::Ready(Some(Err(err))) => Err(err),
    //                    Async::Ready(None) => Ok(Async::Ready(None)),
    //                    Async::NotReady => Ok(Async::NotReady),
    //                }
    //            }
    //            Kind::H2 {
    //                recv: ref mut h2, ..
    //            } => h2
    //                .poll()
    //                .map(|async| {
    //                    async.map(|opt| {
    //                        opt.map(|bytes| {
    //                            let _ = h2.release_capacity().release_capacity(bytes.len());
    //                            Packet::from(bytes)
    //                        })
    //                    })
    //                })
    //                .map_err(::Error::new_body),
    //            Kind::Wrapped(ref mut s) => s.poll().map_err(crate::Error::new_body),
    //        }
    //    }
}

impl Default for Message {
    /// Returns [`Message::empty()`](Message::empty).
    #[inline]
    fn default() -> Message {
        Message::empty()
    }
}

impl Message {
    fn poll_data(&mut self) -> Poll<Result<Option<Packet>, Error>> {
        unimplemented!()
        //        self.poll_eof()
    }

    fn poll_trailers(&mut self) -> Poll<Result<Option<HeaderMap>, Error>> {
        unimplemented!()
        //        match self.kind {
        //            Kind::H2 {
        //                recv: ref mut h2, ..
        //            } => h2.poll_trailers().map_err(::Error::new_h2),
        //            _ => Ok(Async::Ready(None)),
        //        }
    }

    fn is_end_stream(&self) -> bool {
        unimplemented!()
        //        match self.kind {
        //            Kind::Once(ref val) => val.is_none(),
        //            Kind::Chan { content_length, .. } => content_length == Some(0),
        //            Kind::H2 { recv: ref h2, .. } => h2.is_end_stream(),
        //            Kind::Wrapped(..) => false,
        //        }
    }

    fn content_length(&self) -> Option<u64> {
        unimplemented!()
        //        match self.kind {
        //            Kind::Once(Some(ref val)) => Some(val.len() as u64),
        //            Kind::Once(None) => Some(0),
        //            Kind::Wrapped(..) => None,
        //            Kind::Chan { content_length, .. } /*| Kind::H2 { content_length, .. }*/ => content_length,
        //        }
    }

    // We can improve the performance of `Message` when we know it is a Once kind.
    #[doc(hidden)]
    fn __scamp_full_data(&mut self, _: FullDataArg) -> FullDataRet<Packet> {
        unimplemented!()
        //        match self.kind {
        //            Kind::Once(ref mut val) => FullDataRet(val.take()),
        //            _ => FullDataRet(None),
        //        }
    }
}

impl Stream for Message {
    type Item = Packet;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        todo!()
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Message").finish()
    }
}

//impl Sender {
//    /// Check to see if this `Sender` can send more data.
//    pub fn poll_ready(&mut self) -> Poll<(), Error> {
//        match self.abort_tx.poll_cancel() {
//            Ok(Async::Ready(())) | Err(_) => return Err(Error::new_closed()),
//            Ok(Async::NotReady) => (),
//        }
//
//        self.tx.poll_ready().map_err(|_| Error::new_closed())
//    }
//
//    /// Sends data on this channel.
//    ///
//    /// This should be called after `poll_ready` indicated the channel
//    /// could accept another `Packet`.
//    ///
//    /// Returns `Err(Packet)` if the channel could not (currently) accept
//    /// another `Packet`.
//    pub fn send_data(&mut self, chunk: Packet) -> Result<(), Packet> {
//        self.tx
//            .try_send(Ok(chunk))
//            .map_err(|err| err.into_inner().expect("just sent Ok"))
//    }
//
//    /// Aborts the body in an abnormal fashion.
//    pub fn abort(self) {
//        let _ = self.abort_tx.send(());
//    }
//
//    pub(crate) fn send_error(&mut self, err: Error) {
//        let _ = self.tx.try_send(Err(err));
//    }
//}

//impl From<Packet> for Message {
//    #[inline]
//    fn from(chunk: Packet) -> Message {
//        if chunk.is_empty() {
//            Message::empty()
//        } else {
//            Message::new(Kind::Once(Some(chunk)))
//        }
//    }
//}

//impl
//From<Box<Stream<Item = Packet, Error = Box<::std::error::Error + Send + Sync>> + Send + 'static>>
//for Message
//{
//    #[inline]
//    fn from(
//        stream: Box<
//            Stream<Item = Packet, Error = Box<::std::error::Error + Send + Sync>> + Send + 'static,
//        >,
//    ) -> Message {
//        Message::new(Kind::Wrapped(stream))
//    }
//}

//impl From<Bytes> for Message {
//    #[inline]
//    fn from(bytes: Bytes) -> Message {
//        Message::from(Packet::from(bytes))
//    }
//}
//
//impl From<Vec<u8>> for Message {
//    #[inline]
//    fn from(vec: Vec<u8>) -> Message {
//        Message::from(Packet::from(vec))
//    }
//}
//
//impl From<&'static [u8]> for Message {
//    #[inline]
//    fn from(slice: &'static [u8]) -> Message {
//        Message::from(Packet::from(slice))
//    }
//}
//
//impl From<Cow<'static, [u8]>> for Message {
//    #[inline]
//    fn from(cow: Cow<'static, [u8]>) -> Message {
//        match cow {
//            Cow::Borrowed(b) => Message::from(b),
//            Cow::Owned(o) => Message::from(o),
//        }
//    }
//}

//impl From<String> for Message {
//    #[inline]
//    fn from(s: String) -> Message {
//        Message::from(Packet::from(s.into_bytes()))
//    }
//}
//
//impl From<&'static str> for Message {
//    #[inline]
//    fn from(slice: &'static str) -> Message {
//        Message::from(Packet::from(slice.as_bytes()))
//    }
//}
//
//impl From<Cow<'static, str>> for Message {
//    #[inline]
//    fn from(cow: Cow<'static, str>) -> Message {
//        match cow {
//            Cow::Borrowed(b) => Message::from(b),
//            Cow::Owned(o) => Message::from(o),
//        }
//    }
//}

//#[test]
//fn test_body_stream_concat() {
//    let message = Message::from("hello world");
//
//    let total = message.concat2().wait().unwrap();
//    assert_eq!(total.as_ref(), b"hello world");
//}

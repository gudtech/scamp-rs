use std::error::Error as StdError;
use std::fmt;

use std::future::Future;

use crate::Error;
use crate::Message;
use crate::message::Payload;
use crate::common::Never;


pub enum Kind {
    BlockingRPC(Box<Fn(Message) -> Result<Message,Error>>),
    AsyncRPC,
    Async
}

pub struct Action {
    pub name: String,
    // sector
    // probably other stuff about discovery
    kind: Kind
}

/// Create a `Action` from a function.
///
/// # Example
///
/// ```rust
/// use scamp::action::rpc_action;
/// use scamp::Message;
///
/// let action = rpc_action("foo.test",|msg: Message| {
///     Ok(Message::empty())
/// });
/// ```
pub fn rpc_action<F: 'static>(name: &str, f: F) -> Action
    where
        F: Fn(Message) -> Result<Message, Error>,
{
    Action {
        name: name.to_owned(),
        kind: Kind::BlockingRPC(Box::new(f))
    }
}



impl Action{
    pub fn call_blocking (&self, message: Message) -> Result<Message,Error> {
        use tokio_async_await::compat::{backward,forward};

        tokio::runtime::current_thread::block_on_all( backward::Compat::new(self.call(message) ))
    }
    pub async fn call (&self, message: Message) -> Result<Message,Error> {

        //use futures::future::{lazy, poll_fn};
        use tokio_threadpool::blocking;

        match self.kind {
            Kind::BlockingRPC(ref f) => {
                //TODO - Deal with NotReady error
                // Should probably set up our own threadpool for blocking functions

//                use std::future::Future as NewFuture;
//                use futures::Future as OldFuture;
//
//                // converts from an old style Future to a new style one:
//                fn forward<I,E>(f: impl OldFuture<Item=I, Error=E> + Unpin) -> impl NewFuture<Output=Result<I,E>> {
//                    use tokio_async_await::compat::forward::IntoAwaitable;
//                    f.into_awaitable()
//                }

//NOTE: seems to be some incompatibility between
                // futures::poll::Async<std::result::Result<message::message::Message, error::Error>>
                // and
//                let foo :futures::Poll<Result<Message,Error>,tokio_threadpool::BlockingError> = blocking(move|| {
                    f(message)
//                }).map_err(|_| panic!("the threadpool shut down"));
//
//                let bar = forward(foo);
////
//                use tokio_async_await::compat::forward::IntoAwaitable;
//                let bar = foo.into_awaitable();
//
                //await!(bar)
            },
            _ => unimplemented!()
        }
    }
}

///// Create an `Action` that cannot respond or error.
/////
///// # Example
/////
///// ```rust
///// use scamp::{Message, Request, Response};
///// use scamp::action::action_async;
/////
///// let action = action_async(|req: Message| {
/////     println!("request: {} {}", req.method(), req.uri());
/////     Response::new(Message::from("Hello World"))
///// });
///// ```
//pub fn action_async<F, R, S>(name: &str, f: F) -> ActionAsync<F, R>
//    where
//        F: FnMut(R) -> S,
//        S: Payload,
//{
//    ActionAsync {
//        name: name.to_owned(),
//        f,
//        _req: PhantomData,
//    }
//}

/////// An asynchronous function from `Message` to `Option<Message>`.
////pub trait Action {
////    /// The `Payload` body of the request `scamp::Message`.
////    type ReqMessage: Payload;
////
////    /// The `Payload` body of the response `scamp::Message`.
////    type ResMessage: Payload;
////
////    /// The error type that can occur within this `Action`.
////    ///
////    /// Note: Returning an `Error` to a scamp listener will cause the connection
////    /// to be abruptly aborted. In most cases, it is better to return a `Response`
////    /// with a 4xx or 5xx status code.
////    type Error: Into<Box<StdError + Send + Sync>>;
////
////    /// The `Future` returned by this `Action`.
////    type Future: Future<Item=Self::ResMessage, Error=Self::Error>;
////
////    /// Returns `Ready` when the action is able to process requests.
////    ///
////    /// The implementation of this method is allowed to return a `Ready` even if
////    /// the action is not ready to process. In this case, the future returned
////    /// from `call` will resolve to an error.
////    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
////        Ok(Async::Ready(()))
////    }
////
////    /// Calls this `Action` with a request, returning a `Future` of the response.
////    fn call(&mut self, req: Self::ReqMessage) -> Self::Future;
////
////    fn name (&self) -> String;
////}
//
//

//
//impl<F, ReqMessage, Ret, ResMessage> Action for ActionRpc<F, ReqMessage>
//    where
//        F: FnMut(ReqMessage) -> Ret,
//        ReqMessage: Payload,
//        Ret: IntoFuture<Item=ResMessage>,
//        Ret::Error: Into<Box<StdError + Send + Sync>>,
//        ResMessage: Payload,
//{
//    type ReqMessage = ReqMessage;
//    type ResMessage = ResMessage;
//    type Error = Ret::Error;
//    type Future = Ret::Future;
//
//    fn call(&mut self, req: Self::ReqMessage) -> Self::Future {
//        (self.f)(req).into_future()
//    }
//
//    fn name(&self) -> String {
//        self.name.clone()
//    }
//}
//
//impl<F, R> IntoFuture for ActionRpc<F, R> {
//    type Future = future::FutureResult<Self::Item, Self::Error>;
//    type Item = Self;
//    type Error = Never;
//
//    fn into_future(self) -> Self::Future {
//        future::ok(self)
//    }
//}
//
//impl<F, R> fmt::Debug for ActionRpc<F, R> {
//    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//        f.debug_struct("impl Action")
//            .finish()
//    }
//}
//
//// Not exported from crate as this will likely be replaced with `impl Action`.
//pub struct ActionAsync<F, R> {
//    name: String,
//    f: F,
//    _req: PhantomData<fn(R)>,
//}
//
//impl<F, ReqMessage, ResMessage> Action for ActionAsync<F, ReqMessage>
//    where
//        F: FnMut(ReqMessage) -> ResMessage,
//        ReqMessage: Payload,
//        ResMessage: Payload,
//{
//    type ReqMessage = ReqMessage;
//    type ResMessage = ResMessage;
//    type Error = Never;
//    type Future = future::FutureResult<ResMessage, Never>;
//
//    fn call(&mut self, req: Self::ReqMessage) -> Self::Future {
//        future::ok((self.f)(req))
//    }
//
//    fn name(&self) -> String {
//        self.name.clone()
//    }
//}
//
//impl<F, R> IntoFuture for ActionAsync<F, R> {
//    type Future = future::FutureResult<Self::Item, Self::Error>;
//    type Item = Self;
//    type Error = Never;
//
//    fn into_future(self) -> Self::Future {
//        future::ok(self)
//    }
//}
//
//impl<F, R> fmt::Debug for ActionAsync<F, R> {
//    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//        f.debug_struct("impl Action")
//            .finish()
//    }
//}
//
////#[cfg(test)]
//fn _assert_fn_mut() {
//    fn assert_action<T: Action>(_t: &T) {}
//
//    let mut val = 0;
//
//    let act = action_rpc("test",move |_req: Message| {
//        val += 1;
//        future::ok::<_, Never>(Message::empty())
//    });
//
//    assert_action(&act);
//
//    let act = action_async("test2",move |_req: Message| {
//        val += 1;
//        Message::empty()
//    });
//
//    assert_action(&act);
//}
use std::error::Error as StdError;
use std::fmt;
use std::marker::PhantomData;

use futures::{future, Async, Future, IntoFuture, Poll};

use body::Payload;
use common::Never;
use ::{Request, Response};

/// An asynchronous function from `Message` to `Option<Message>`.
pub trait Action {
    /// The `Payload` body of the request `scamp::Message`.
    type ReqBody: Payload;

    /// The `Payload` body of the response `scamp::Message`.
    type ResBody: Payload;

    /// The error type that can occur within this `Action`.
    ///
    /// Note: Returning an `Error` to a scamp listener will cause the connection
    /// to be abruptly aborted. In most cases, it is better to return a `Response`
    /// with a 4xx or 5xx status code.
    type Error: Into<Box<StdError + Send + Sync>>;

    /// The `Future` returned by this `Action`.
    type Future: Future<Item=Response<Self::ResBody>, Error=Self::Error>;

    /// Returns `Ready` when the action is able to process requests.
    ///
    /// The implementation of this method is allowed to return a `Ready` even if
    /// the action is not ready to process. In this case, the future returned
    /// from `call` will resolve to an error.
    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(Async::Ready(()))
    }

    /// Calls this `Action` with a request, returning a `Future` of the response.
    fn call(&mut self, req: Request<Self::ReqBody>) -> Self::Future;
}


/// Create a `Action` from a function.
///
/// # Example
///
/// ```rust
/// use scamp::{Body, Request, Response, Version};
/// use scamp::action::action_fn;
///
/// let action = action_rpc(|req: Request<Body>| {
///     if req.version() == Version::HTTP_11 {
///         Ok(Response::new(Body::from("Hello World")))
///     } else {
///         // Note: it's usually better to return a Response
///         // with an appropriate StatusCode instead of an Err.
///         Err("not HTTP/1.1, abort connection")
///     }
/// });
/// ```
pub fn action_fn<F, R, S>(f: F) -> ActionRpc<F, R>
    where
        F: FnMut(Request<R>) -> S,
        S: IntoFuture,
{
    ActionRpc {
        f,
        _req: PhantomData,
    }
}

/// Create an `Action` that cannot respond or error.
///
/// # Example
///
/// ```rust
/// use scamp::{Body, Request, Response};
/// use scamp::action::action_async;
///
/// let action = action_async(|req: Request<Body>| {
///     println!("request: {} {}", req.method(), req.uri());
///     Response::new(Body::from("Hello World"))
/// });
/// ```
pub fn action_async<F, R, S>(f: F) -> ActionAsync<F, R>
    where
        F: FnMut(Request<R>) -> Response<S>,
        S: Payload,
{
    ActionAsync {
        f,
        _req: PhantomData,
    }
}

// Not exported from crate as this will likely be replaced with `impl Action`.
pub struct ActionRpc<F, R> {
    f: F,
    _req: PhantomData<fn(R)>,
}

impl<F, ReqBody, Ret, ResBody> Action for ActionRpc<F, ReqBody>
    where
        F: FnMut(Request<ReqBody>) -> Ret,
        ReqBody: Payload,
        Ret: IntoFuture<Item=Response<ResBody>>,
        Ret::Error: Into<Box<StdError + Send + Sync>>,
        ResBody: Payload,
{
    type ReqBody = ReqBody;
    type ResBody = ResBody;
    type Error = Ret::Error;
    type Future = Ret::Future;

    fn call(&mut self, req: Request<Self::ReqBody>) -> Self::Future {
        (self.f)(req).into_future()
    }
}

impl<F, R> IntoFuture for ActionRpc<F, R> {
    type Future = future::FutureResult<Self::Item, Self::Error>;
    type Item = Self;
    type Error = Never;

    fn into_future(self) -> Self::Future {
        future::ok(self)
    }
}

impl<F, R> fmt::Debug for ActionRpc<F, R> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("impl Action")
            .finish()
    }
}

// Not exported from crate as this will likely be replaced with `impl Action`.
pub struct ActionAsync<F, R> {
    f: F,
    _req: PhantomData<fn(R)>,
}

impl<F, ReqBody, ResBody> Action for ActionAsync<F, ReqBody>
    where
        F: FnMut(Request<ReqBody>) -> Response<ResBody>,
        ReqBody: Payload,
        ResBody: Payload,
{
    type ReqBody = ReqBody;
    type ResBody = ResBody;
    type Error = Never;
    type Future = future::FutureResult<Response<ResBody>, Never>;

    fn call(&mut self, req: Request<Self::ReqBody>) -> Self::Future {
        future::ok((self.f)(req))
    }
}

impl<F, R> IntoFuture for ActionAsync<F, R> {
    type Future = future::FutureResult<Self::Item, Self::Error>;
    type Item = Self;
    type Error = Never;

    fn into_future(self) -> Self::Future {
        future::ok(self)
    }
}

impl<F, R> fmt::Debug for ActionAsync<F, R> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("impl Action")
            .finish()
    }
}

//#[cfg(test)]
fn _assert_fn_mut() {
    fn assert_action<T: Action>(_t: &T) {}

    let mut val = 0;

    let svc = action_rpc(move |_req: Request<::Body>| {
        val += 1;
        future::ok::<_, Never>(Response::new(::Body::empty()))
    });

    assert_action(&svc);

    let svc = action_async(move |_req: Request<::Body>| {
        val += 1;
        Response::new(::Body::empty())
    });

    assert_action(&svc);
}
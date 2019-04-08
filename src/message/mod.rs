use futures::Future;
use futures::stream::Stream;
use crate::packet::Packet;
mod body;

pub struct Message<T> {
    headers: MessageHeaders,
    body: T
}

impl Message {
    pub async fn readAll (&self) -> () {
        unimplemented!()
//        let acc = [];
//        this.on('data', (d) => acc.push(d));
//        this.on('end', () => {
//            if (this.error)
//            return callback('transport', this.error, null);
//            if (this.header.error_code)
//            return callback(this.header.error_code, this.header.error, null);
//
//            var resp = Buffer.concat(acc).toString('utf8');
//            try {
//                resp = JSON.parse(resp);
//            } catch (e) {}
//
//            if (resp === undefined)
//            return callback('transport', 'failed to parse JSON response', null);
//
//            return callback(null, null, resp);
//        });
    }
}

impl Stream for Message {
    type Item = Packet;

    // The stream will never yield an error
    type Error = ();

    fn poll(&mut self) -> futures::Poll<Option<Packet>, ()> {
//        let curr = self.curr;
//        let next = curr + self.next;
//
//        self.curr = self.next;
//        self.next = next;
//
//        Ok(futures::Async::Ready(Some(curr)))
        unimplemented!()
    }
}


/// Represents an HTTP request.
///
/// An HTTP request consists of a head and a potentially optional body. The body
/// component is generic, enabling arbitrary types to represent the HTTP body.
/// For example, the body could be `Vec<u8>`, a `Stream` of byte chunks, or a
/// value that has been deserialized.
///
/// # Examples
///
/// Creating a `Request` to send
///
/// ```no_run
/// use http::{Request, Response};
///
/// let mut request = Request::builder();
/// request.uri("https://www.rust-lang.org/")
///        .header("User-Agent", "my-awesome-agent/1.0");
///
/// if needs_awesome_header() {
///     request.header("Awesome", "yes");
/// }
///
/// let response = send(request.body(()).unwrap());
///
/// # fn needs_awesome_header() -> bool {
/// #     true
/// # }
/// #
/// fn send(req: Request<()>) -> Response<()> {
///     // ...
/// # panic!()
/// }
/// ```
///
/// Inspecting a request to see what was sent.
///
/// ```
/// use http::{Request, Response, StatusCode};
///
/// fn respond_to(req: Request<()>) -> http::Result<Response<()>> {
///     if req.uri() != "/awesome-url" {
///         return Response::builder()
///             .status(StatusCode::NOT_FOUND)
///             .body(())
///     }
///
///     let has_awesome_header = req.headers().contains_key("Awesome");
///     let body = req.body();
///
///     // ...
/// # panic!()
/// }
/// ```
///
/// Deserialize a request of bytes via json:
///
/// ```
/// # extern crate serde;
/// # extern crate serde_json;
/// # extern crate http;
/// use http::Request;
/// use serde::de;
///
/// fn deserialize<T>(req: Request<Vec<u8>>) -> serde_json::Result<Request<T>>
///     where for<'de> T: de::Deserialize<'de>,
/// {
///     let (parts, body) = req.into_parts();
///     let body = serde_json::from_slice(&body)?;
///     Ok(Request::from_parts(parts, body))
/// }
/// #
/// # fn main() {}
/// ```
///
/// Or alternatively, serialize the body of a request to json
///
/// ```
/// # extern crate serde;
/// # extern crate serde_json;
/// # extern crate http;
/// use http::Request;
/// use serde::ser;
///
/// fn serialize<T>(req: Request<T>) -> serde_json::Result<Request<Vec<u8>>>
///     where T: ser::Serialize,
/// {
///     let (parts, body) = req.into_parts();
///     let body = serde_json::to_vec(&body)?;
///     Ok(Request::from_parts(parts, body))
/// }
/// #
/// # fn main() {}
/// ```
pub struct Request<T> {
    head: Parts,
    body: T,
}

/// Component parts of an HTTP `Request`
///
/// The HTTP request head consists of a method, uri, version, and a set of
/// header fields.
pub struct Parts {
    /// The request's method
    pub method: Method,

    /// The request's URI
    pub uri: Uri,

    /// The request's version
    pub version: Version,

    /// The request's headers
    pub headers: HeaderMap<HeaderValue>,

    /// The request's extensions
    pub extensions: Extensions,

    _priv: (),
}

/// An HTTP request builder
///
/// This type can be used to construct an instance or `Request`
/// through a builder-like pattern.
#[derive(Debug)]
pub struct Builder {
    head: Option<Parts>,
    err: Option<Error>,
}

impl Request<()> {
    /// Creates a new builder-style object to manufacture a `Request`
    ///
    /// This method returns an instance of `Builder` which can be used to
    /// create a `Request`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let request = Request::builder()
    ///     .method("GET")
    ///     .uri("https://www.rust-lang.org/")
    ///     .header("X-Custom-Foo", "Bar")
    ///     .body(())
    ///     .unwrap();
    /// ```
    #[inline]
    pub fn builder() -> Builder {
        Builder::new()
    }


    /// Creates a new `Builder` initialized with a GET method and the given URI.
    ///
    /// This method returns an instance of `Builder` which can be used to
    /// create a `Request`.
    ///
    /// # Example
    ///
    /// ```
    /// # use http::*;
    ///
    /// let request = Request::get("https://www.rust-lang.org/")
    ///     .body(())
    ///     .unwrap();
    /// ```
    pub fn get<T>(uri: T) -> Builder
        where Uri: HttpTryFrom<T> {
        let mut b = Builder::new();
        b.method(Method::GET).uri(uri);
        b
    }

    /// Creates a new `Builder` initialized with a PUT method and the given URI.
    ///
    /// This method returns an instance of `Builder` which can be used to
    /// create a `Request`.
    ///
    /// # Example
    ///
    /// ```
    /// # use http::*;
    ///
    /// let request = Request::put("https://www.rust-lang.org/")
    ///     .body(())
    ///     .unwrap();
    /// ```
    pub fn put<T>(uri: T) -> Builder
        where Uri: HttpTryFrom<T> {
        let mut b = Builder::new();
        b.method(Method::PUT).uri(uri);
        b
    }

    /// Creates a new `Builder` initialized with a POST method and the given URI.
    ///
    /// This method returns an instance of `Builder` which can be used to
    /// create a `Request`.
    ///
    /// # Example
    ///
    /// ```
    /// # use http::*;
    ///
    /// let request = Request::post("https://www.rust-lang.org/")
    ///     .body(())
    ///     .unwrap();
    /// ```
    pub fn post<T>(uri: T) -> Builder
        where Uri: HttpTryFrom<T> {
        let mut b = Builder::new();
        b.method(Method::POST).uri(uri);
        b
    }

    /// Creates a new `Builder` initialized with a DELETE method and the given URI.
    ///
    /// This method returns an instance of `Builder` which can be used to
    /// create a `Request`.
    ///
    /// # Example
    ///
    /// ```
    /// # use http::*;
    ///
    /// let request = Request::delete("https://www.rust-lang.org/")
    ///     .body(())
    ///     .unwrap();
    /// ```
    pub fn delete<T>(uri: T) -> Builder
        where Uri: HttpTryFrom<T> {
        let mut b = Builder::new();
        b.method(Method::DELETE).uri(uri);
        b
    }

    /// Creates a new `Builder` initialized with an OPTIONS method and the given URI.
    ///
    /// This method returns an instance of `Builder` which can be used to
    /// create a `Request`.
    ///
    /// # Example
    ///
    /// ```
    /// # use http::*;
    ///
    /// let request = Request::options("https://www.rust-lang.org/")
    ///     .body(())
    ///     .unwrap();
    /// # assert_eq!(*request.method(), Method::OPTIONS);
    /// ```
    pub fn options<T>(uri: T) -> Builder
        where Uri: HttpTryFrom<T> {
        let mut b = Builder::new();
        b.method(Method::OPTIONS).uri(uri);
        b
    }

    /// Creates a new `Builder` initialized with a HEAD method and the given URI.
    ///
    /// This method returns an instance of `Builder` which can be used to
    /// create a `Request`.
    ///
    /// # Example
    ///
    /// ```
    /// # use http::*;
    ///
    /// let request = Request::head("https://www.rust-lang.org/")
    ///     .body(())
    ///     .unwrap();
    /// ```
    pub fn head<T>(uri: T) -> Builder
        where Uri: HttpTryFrom<T> {
        let mut b = Builder::new();
        b.method(Method::HEAD).uri(uri);
        b
    }

    /// Creates a new `Builder` initialized with a CONNECT method and the given URI.
    ///
    /// This method returns an instance of `Builder` which can be used to
    /// create a `Request`.
    ///
    /// # Example
    ///
    /// ```
    /// # use http::*;
    ///
    /// let request = Request::connect("https://www.rust-lang.org/")
    ///     .body(())
    ///     .unwrap();
    /// ```
    pub fn connect<T>(uri: T) -> Builder
        where Uri: HttpTryFrom<T> {
        let mut b = Builder::new();
        b.method(Method::CONNECT).uri(uri);
        b
    }

    /// Creates a new `Builder` initialized with a PATCH method and the given URI.
    ///
    /// This method returns an instance of `Builder` which can be used to
    /// create a `Request`.
    ///
    /// # Example
    ///
    /// ```
    /// # use http::*;
    ///
    /// let request = Request::patch("https://www.rust-lang.org/")
    ///     .body(())
    ///     .unwrap();
    /// ```
    pub fn patch<T>(uri: T) -> Builder
        where Uri: HttpTryFrom<T> {
        let mut b = Builder::new();
        b.method(Method::PATCH).uri(uri);
        b
    }

    /// Creates a new `Builder` initialized with a TRACE method and the given URI.
    ///
    /// This method returns an instance of `Builder` which can be used to
    /// create a `Request`.
    ///
    /// # Example
    ///
    /// ```
    /// # use http::*;
    ///
    /// let request = Request::trace("https://www.rust-lang.org/")
    ///     .body(())
    ///     .unwrap();
    /// ```
    pub fn trace<T>(uri: T) -> Builder
        where Uri: HttpTryFrom<T> {
        let mut b = Builder::new();
        b.method(Method::TRACE).uri(uri);
        b
    }
}

impl<T> Request<T> {
    /// Creates a new blank `Request` with the body
    ///
    /// The component parts of this request will be set to their default, e.g.
    /// the GET method, no headers, etc.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let request = Request::new("hello world");
    ///
    /// assert_eq!(*request.method(), Method::GET);
    /// assert_eq!(*request.body(), "hello world");
    /// ```
    #[inline]
    pub fn new(body: T) -> Request<T> {
        Request {
            head: Parts::new(),
            body: body,
        }
    }

    /// Creates a new `Request` with the given components parts and body.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let request = Request::new("hello world");
    /// let (mut parts, body) = request.into_parts();
    /// parts.method = Method::POST;
    ///
    /// let request = Request::from_parts(parts, body);
    /// ```
    #[inline]
    pub fn from_parts(parts: Parts, body: T) -> Request<T> {
        Request {
            head: parts,
            body: body,
        }
    }

    /// Returns a reference to the associated HTTP method.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let request: Request<()> = Request::default();
    /// assert_eq!(*request.method(), Method::GET);
    /// ```
    #[inline]
    pub fn method(&self) -> &Method {
        &self.head.method
    }

    /// Returns a mutable reference to the associated HTTP method.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let mut request: Request<()> = Request::default();
    /// *request.method_mut() = Method::PUT;
    /// assert_eq!(*request.method(), Method::PUT);
    /// ```
    #[inline]
    pub fn method_mut(&mut self) -> &mut Method {
        &mut self.head.method
    }

    /// Returns a reference to the associated URI.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let request: Request<()> = Request::default();
    /// assert_eq!(*request.uri(), *"/");
    /// ```
    #[inline]
    pub fn uri(&self) -> &Uri {
        &self.head.uri
    }

    /// Returns a mutable reference to the associated URI.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let mut request: Request<()> = Request::default();
    /// *request.uri_mut() = "/hello".parse().unwrap();
    /// assert_eq!(*request.uri(), *"/hello");
    /// ```
    #[inline]
    pub fn uri_mut(&mut self) -> &mut Uri {
        &mut self.head.uri
    }

    /// Returns the associated version.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let request: Request<()> = Request::default();
    /// assert_eq!(request.version(), Version::HTTP_11);
    /// ```
    #[inline]
    pub fn version(&self) -> Version {
        self.head.version
    }

    /// Returns a mutable reference to the associated version.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let mut request: Request<()> = Request::default();
    /// *request.version_mut() = Version::HTTP_2;
    /// assert_eq!(request.version(), Version::HTTP_2);
    /// ```
    #[inline]
    pub fn version_mut(&mut self) -> &mut Version {
        &mut self.head.version
    }

    /// Returns a reference to the associated header field map.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let request: Request<()> = Request::default();
    /// assert!(request.headers().is_empty());
    /// ```
    #[inline]
    pub fn headers(&self) -> &HeaderMap<HeaderValue> {
        &self.head.headers
    }

    /// Returns a mutable reference to the associated header field map.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// # use http::header::*;
    /// let mut request: Request<()> = Request::default();
    /// request.headers_mut().insert(HOST, HeaderValue::from_static("world"));
    /// assert!(!request.headers().is_empty());
    /// ```
    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        &mut self.head.headers
    }


    /// Returns a reference to the associated extensions.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let request: Request<()> = Request::default();
    /// assert!(request.extensions().get::<i32>().is_none());
    /// ```
    #[inline]
    pub fn extensions(&self) -> &Extensions {
        &self.head.extensions
    }

    /// Returns a mutable reference to the associated extensions.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// # use http::header::*;
    /// let mut request: Request<()> = Request::default();
    /// request.extensions_mut().insert("hello");
    /// assert_eq!(request.extensions().get(), Some(&"hello"));
    /// ```
    #[inline]
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.head.extensions
    }

    /// Returns a reference to the associated HTTP body.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let request: Request<String> = Request::default();
    /// assert!(request.body().is_empty());
    /// ```
    #[inline]
    pub fn body(&self) -> &T {
        &self.body
    }

    /// Returns a mutable reference to the associated HTTP body.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let mut request: Request<String> = Request::default();
    /// request.body_mut().push_str("hello world");
    /// assert!(!request.body().is_empty());
    /// ```
    #[inline]
    pub fn body_mut(&mut self) -> &mut T {
        &mut self.body
    }


    /// Consumes the request, returning just the body.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::Request;
    /// let request = Request::new(10);
    /// let body = request.into_body();
    /// assert_eq!(body, 10);
    /// ```
    #[inline]
    pub fn into_body(self) -> T {
        self.body
    }

    /// Consumes the request returning the head and body parts.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let request = Request::new(());
    /// let (parts, body) = request.into_parts();
    /// assert_eq!(parts.method, Method::GET);
    /// ```
    #[inline]
    pub fn into_parts(self) -> (Parts, T) {
        (self.head, self.body)
    }

    /// Consumes the request returning a new request with body mapped to the
    /// return type of the passed in function.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// let request = Request::builder().body("some string").unwrap();
    /// let mapped_request: Request<&[u8]> = request.map(|b| {
    ///   assert_eq!(b, "some string");
    ///   b.as_bytes()
    /// });
    /// assert_eq!(mapped_request.body(), &"some string".as_bytes());
    /// ```
    #[inline]
    pub fn map<F, U>(self, f: F) -> Request<U>
        where F: FnOnce(T) -> U
    {
        Request { body: f(self.body), head: self.head }
    }
}

impl<T: Default> Default for Request<T> {
    fn default() -> Request<T> {
        Request::new(T::default())
    }
}

impl<T: fmt::Debug> fmt::Debug for Request<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Request")
            .field("method", self.method())
            .field("uri", self.uri())
            .field("version", &self.version())
            .field("headers", self.headers())
            // omits Extensions because not useful
            .field("body", self.body())
            .finish()
    }
}

impl Parts {
    /// Creates a new default instance of `Parts`
    fn new() -> Parts {
        Parts{
            method: Method::default(),
            uri: Uri::default(),
            version: Version::default(),
            headers: HeaderMap::default(),
            extensions: Extensions::default(),
            _priv: (),
        }
    }
}

impl fmt::Debug for Parts {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Parts")
            .field("method", &self.method)
            .field("uri", &self.uri)
            .field("version", &self.version)
            .field("headers", &self.headers)
            // omits Extensions because not useful
            // omits _priv because not useful
            .finish()
    }
}

impl Builder {
    /// Creates a new default instance of `Builder` to construct a `Request`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    ///
    /// let req = request::Builder::new()
    ///     .method("POST")
    ///     .body(())
    ///     .unwrap();
    /// ```
    #[inline]
    pub fn new() -> Builder {
        Builder::default()
    }

    /// Set the HTTP method for this request.
    ///
    /// This function will configure the HTTP method of the `Request` that will
    /// be returned from `Builder::build`.
    ///
    /// By default this is `GET`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    ///
    /// let req = Request::builder()
    ///     .method("POST")
    ///     .body(())
    ///     .unwrap();
    /// ```
    pub fn method<T>(&mut self, method: T) -> &mut Builder
        where Method: HttpTryFrom<T>,
    {
        if let Some(head) = head(&mut self.head, &self.err) {
            match HttpTryFrom::try_from(method) {
                Ok(s) => head.method = s,
                Err(e) => self.err = Some(e.into()),
            }
        }
        self
    }

    /// Get the HTTP Method for this request.
    ///
    /// By default this is `GET`.
    /// if builder has error, returns None.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    ///
    /// let mut req = Request::builder();
    /// assert_eq!(req.method_ref(),Some(&Method::GET));
    /// req.method("POST");
    /// assert_eq!(req.method_ref(),Some(&Method::POST));
    /// req.method("DELETE");
    /// assert_eq!(req.method_ref(),Some(&Method::DELETE));
    /// ```
    pub fn method_ref(&self) -> Option<&Method>
    {
        if self.err.is_some() {
            return None
        }
        match self.head {
            Some(ref head) => Some(&head.method),
            None => None
        }
    }

    /// Set the URI for this request.
    ///
    /// This function will configure the URI of the `Request` that will
    /// be returned from `Builder::build`.
    ///
    /// By default this is `/`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    ///
    /// let req = Request::builder()
    ///     .uri("https://www.rust-lang.org/")
    ///     .body(())
    ///     .unwrap();
    /// ```
    pub fn uri<T>(&mut self, uri: T) -> &mut Builder
        where Uri: HttpTryFrom<T>,
    {
        if let Some(head) = head(&mut self.head, &self.err) {
            match HttpTryFrom::try_from(uri) {
                Ok(s) => head.uri = s,
                Err(e) => self.err = Some(e.into()),
            }
        }
        self
    }

    /// Get the URI for this request
    ///
    /// By default this is `/`
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    ///
    /// let mut req = Request::builder();
    /// assert_eq!(req.uri_ref().unwrap().to_string(), "/" );
    /// req.uri("https://www.rust-lang.org/");
    /// assert_eq!(req.uri_ref().unwrap().to_string(), "https://www.rust-lang.org/" );
    /// ```
    pub fn uri_ref(&self) -> Option<&Uri>
    {
        if self.err.is_some() {
            return None;
        }
        match self.head
            {
                Some(ref head) => Some(&head.uri),
                None => None
            }
    }

    /// Set the HTTP version for this request.
    ///
    /// This function will configure the HTTP version of the `Request` that
    /// will be returned from `Builder::build`.
    ///
    /// By default this is HTTP/1.1
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    ///
    /// let req = Request::builder()
    ///     .version(Version::HTTP_2)
    ///     .body(())
    ///     .unwrap();
    /// ```
    pub fn version(&mut self, version: Version) -> &mut Builder {
        if let Some(head) = head(&mut self.head, &self.err) {
            head.version = version;
        }
        self
    }

    /// Appends a header to this request builder.
    ///
    /// This function will append the provided key/value as a header to the
    /// internal `HeaderMap` being constructed. Essentially this is equivalent
    /// to calling `HeaderMap::append`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    /// # use http::header::HeaderValue;
    ///
    /// let req = Request::builder()
    ///     .header("Accept", "text/html")
    ///     .header("X-Custom-Foo", "bar")
    ///     .body(())
    ///     .unwrap();
    /// ```
    pub fn header<K, V>(&mut self, key: K, value: V) -> &mut Builder
        where HeaderName: HttpTryFrom<K>,
              HeaderValue: HttpTryFrom<V>
    {
        if let Some(head) = head(&mut self.head, &self.err) {
            match <HeaderName as HttpTryFrom<K>>::try_from(key) {
                Ok(key) => {
                    match <HeaderValue as HttpTryFrom<V>>::try_from(value) {
                        Ok(value) => { head.headers.append(key, value); }
                        Err(e) => self.err = Some(e.into()),
                    }
                },
                Err(e) => self.err = Some(e.into()),
            };
        }
        self
    }

    /// Get header on this request builder.
    /// when builder has error returns None
    ///
    /// # Example
    ///
    /// ```
    /// # use http::*;
    /// # use http::header::HeaderValue;
    /// # use http::request::Builder;
    /// let mut req = Request::builder();
    /// req.header("Accept", "text/html")
    ///    .header("X-Custom-Foo", "bar");
    /// let headers = req.headers_ref().unwrap();
    /// assert_eq!( headers["Accept"], "text/html" );
    /// assert_eq!( headers["X-Custom-Foo"], "bar" );
    /// ```
    pub fn headers_ref(&self) -> Option<&HeaderMap<HeaderValue>> {
        if self.err.is_some() {
            return None;
        }
        match self.head
            {
                Some(ref head) => Some(&head.headers),
                None => None
            }
    }

    /// Adds an extension to this builder
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    ///
    /// let req = Request::builder()
    ///     .extension("My Extension")
    ///     .body(())
    ///     .unwrap();
    ///
    /// assert_eq!(req.extensions().get::<&'static str>(),
    ///            Some(&"My Extension"));
    /// ```
    pub fn extension<T>(&mut self, extension: T) -> &mut Builder
        where T: Any + Send + Sync + 'static,
    {
        if let Some(head) = head(&mut self.head, &self.err) {
            head.extensions.insert(extension);
        }
        self
    }

    fn take_parts(&mut self) -> Result<Parts> {
        let ret = self.head.take().expect("cannot reuse request builder");
        if let Some(e) = self.err.take() {
            return Err(e)
        }
        Ok(ret)
    }

    /// "Consumes" this builder, using the provided `body` to return a
    /// constructed `Request`.
    ///
    /// # Errors
    ///
    /// This function may return an error if any previously configured argument
    /// failed to parse or get converted to the internal representation. For
    /// example if an invalid `head` was specified via `header("Foo",
    /// "Bar\r\n")` the error will be returned when this function is called
    /// rather than when `header` was called.
    ///
    /// # Panics
    ///
    /// This method will panic if the builder is reused. The `body` function can
    /// only be called once.
    ///
    /// # Examples
    ///
    /// ```
    /// # use http::*;
    ///
    /// let request = Request::builder()
    ///     .body(())
    ///     .unwrap();
    /// ```
    pub fn body<T>(&mut self, body: T) -> Result<Request<T>> {
        Ok(Request {
            head: self.take_parts()?,
            body: body,
        })
    }
}

fn head<'a>(head: &'a mut Option<Parts>, err: &Option<Error>)
            -> Option<&'a mut Parts>
{
    if err.is_some() {
        return None
    }
    head.as_mut()
}

impl Default for Builder {
    #[inline]
    fn default() -> Builder {
        Builder {
            head: Some(Parts::new()),
            err: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_can_map_a_body_from_one_type_to_another() {
        let request= Request::builder().body("some string").unwrap();
        let mapped_request = request.map(|s| {
            assert_eq!(s, "some string");
            123u32
        });
        assert_eq!(mapped_request.body(), &123u32);
    }
}
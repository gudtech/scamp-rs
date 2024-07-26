use std::collections::BTreeMap;

use crate::discovery::service_registry::ActionEntry;
use anyhow::Result;
use std::io::Cursor;
use tokio::io::AsyncRead;
use tokio::io::BufReader;

pub mod beepish;
pub mod mock;

pub struct Request<'a> {
    pub action: &'a ActionEntry,
    pub headers: BTreeMap<String, String>,
    pub body: Box<dyn AsyncRead + Unpin>,
}
pub struct Response {
    pub headers: BTreeMap<String, String>,
    pub body: Box<dyn AsyncRead + Unpin>,
}

pub trait Client {
    async fn request<'a>(
        &self,
        action: &'a ActionEntry,
        headers: BTreeMap<String, String>,
        body: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Result<Response>;
}

// pub trait IntoAsyncRead {
//     fn into_async_read(self) -> Box<dyn AsyncRead + Unpin + 'static>;
// }

// // impl<T: AsyncRead + Unpin + 'static> IntoAsyncRead for T {
// //     fn into_async_read(self) -> Box<dyn AsyncRead + Unpin + 'static> {
// //         Box::new(self)
// //     }
// // }
// impl<T: AsyncRead + Unpin + 'static> IntoAsyncRead for T {
//     fn into_async_read(self) -> Box<dyn AsyncRead + Unpin + 'static> {
//         Box::new(self)
//     }
// }

// impl IntoAsyncRead for Vec<u8> {
//     fn into_async_read(self) -> Box<dyn AsyncRead + Unpin + 'static> {
//         Box::new(BufReader::new(Cursor::new(self)))
//     }
// }

// impl IntoAsyncRead for String {
//     fn into_async_read(self) -> Box<dyn AsyncRead + Unpin + 'static> {
//         Box::new(BufReader::new(Cursor::new(self.into_bytes())))
//     }
// }

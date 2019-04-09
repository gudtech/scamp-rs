use std::collections::HashMap;

use std::fmt;

use bytes::{Buf, Bytes};

/// A piece of a [`Message`].
#[derive(Debug)]
pub enum Packet {
    HEADER{ id: u32, values: HashMap<String,String> },
    DATA{ id: u32, bytes: Bytes },
    EOF,
    TXERR,
    ACK,
    PING
}

//
//impl Packet {
//    /// Converts this `Packet` directly into the `Bytes` type without copies.
//    ///
//    /// This is simply an inherent alias for `Bytes::from(chunk)`, which exists,
//    /// but doesn't appear in rustdocs.
//    #[inline]
//    pub fn into_bytes(self) -> Bytes {
//        self.into()
//    }
//
//}

//impl Buf for Packet {
//    #[inline]
//    fn remaining(&self) -> usize {
//        //perf: Bytes::len() isn't inline yet,
//        //so it's slightly slower than checking
//        //the length of the slice.
//        self.bytes().len()
//    }
//
//    #[inline]
//    fn bytes(&self) -> &[u8] {
//        &self.bytes
//    }
//
//    #[inline]
//    fn advance(&mut self, cnt: usize) {
//        self.bytes.advance(cnt);
//    }
//}
//
//impl From<Vec<u8>> for Packet {
//    #[inline]
//    fn from(v: Vec<u8>) -> Packet {
//        Packet::from(Bytes::from(v))
//    }
//}
//
//impl From<&'static [u8]> for Packet {
//    #[inline]
//    fn from(slice: &'static [u8]) -> Packet {
//        Packet::from(Bytes::from_static(slice))
//    }
//}
//
//impl From<String> for Packet {
//    #[inline]
//    fn from(s: String) -> Packet {
//        s.into_bytes().into()
//    }
//}
//
//impl From<&'static str> for Packet {
//    #[inline]
//    fn from(slice: &'static str) -> Packet {
//        slice.as_bytes().into()
//    }
//}
//
//impl From<Bytes> for Packet {
//    #[inline]
//    fn from(bytes: Bytes) -> Packet {
//        Packet {
//            bytes: bytes,
//        }
//    }
//}
//
//impl From<Packet> for Bytes {
//    #[inline]
//    fn from(chunk: Packet) -> Bytes {
////        chunk.bytes
//        unimplemented!()
//    }
//}
//
//impl ::std::ops::Deref for Packet {
//    type Target = [u8];
//
//    #[inline]
//    fn deref(&self) -> &Self::Target {
//        self.as_ref()
//    }
//}
//
//impl AsRef<[u8]> for Packet {
//    #[inline]
//    fn as_ref(&self) -> &[u8] {
//        &self.bytes
//    }
//}
//
//impl fmt::Debug for Packet {
//    #[inline]
//    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//        fmt::Debug::fmt(self.as_ref(), f)
//    }
//}
//
//#[cfg(test)]
//mod tests {
//    #[cfg(feature = "nightly")]
//    use test::Bencher;
//
//    #[cfg(feature = "nightly")]
//    #[bench]
//    fn bench_chunk_static_buf(b: &mut Bencher) {
//        use bytes::BufMut;
//
//        let s = "Hello, World!";
//        b.bytes = s.len() as u64;
//
//        let mut dst = Vec::with_capacity(128);
//
//        b.iter(|| {
//            let chunk = ::Packet::from(s);
//            dst.put(chunk);
//            ::test::black_box(&dst);
//            unsafe { dst.set_len(0); }
//        })
//    }
//}

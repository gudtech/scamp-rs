
#[derive(Debug)]
pub struct Error{
    e: std::io::Error
}

//
//impl Error {
//    pub(crate) fn new_body<E: Into<Cause>>(cause: E) -> Error {
//        unimplemented!()
////        Error::new(Kind::Body, Some(cause.into()))
//    }
//}

impl From <std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error{
            e: e
        }
    }
}

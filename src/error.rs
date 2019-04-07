
pub struct Error{
    e: std::io::Error
}

impl From <std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error{
            e: e
        }
    }
}
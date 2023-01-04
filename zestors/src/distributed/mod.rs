mod conn;
mod system;

pub use conn::*;
pub use system::*;

use crate::request::SendRecvError;

#[derive(Debug)]
pub enum ConnectError {
    Todo,
    NodeDown
}

impl<T> From<SendRecvError<T>> for ConnectError {
    fn from(_: SendRecvError<T>) -> Self {
        ConnectError::NodeDown
    }
}
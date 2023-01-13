// mod conn;
// mod system;

// use crate::core::SendRecvError;
// pub use conn::*;
// pub use system::*;

// #[derive(Debug)]
// pub enum ConnectError {
//     Todo,
//     NodeDown,
// }

// impl<T> From<SendRecvError<T>> for ConnectError {
//     fn from(_: SendRecvError<T>) -> Self {
//         ConnectError::NodeDown
//     }
// }

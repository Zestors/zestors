use serde::{de::DeserializeOwned, Serialize};

use crate::{core::*, distr::distr_addr::RemoteMsg};

// struct LocalSnd<T> {
//     sender: oneshot::Sender<T>,
// }

// impl<T> LocalSnd<T> {
//     pub fn send(self, msg: T) -> Result<(), SendError<T>> {
//         Ok(self.sender.send(msg)?)
//     }
// }

// struct RemoteSnd<T> {
//     sender: oneshot::Sender<RemoteMsg<T>>,
// }

// impl<T> RemoteSnd<T> {
//     pub fn send(self, msg: impl Into<RemoteMsg<T>>) -> Result<(), SendError<T>> {
//         match self.sender.send(msg.into()) {
//             Ok(()) => Ok(()),
//             Err(_e) => todo!(),
//         }
//     }
// }

// enum AnySnd<T> {
//     Local(LocalSnd<T>),
//     Remote(RemoteSnd<T>),
// }

// impl<T> AnySnd<T> {
//     pub fn send_ref(self, msg: T) -> Result<(), SendError<T>>
//     where
//         T: Into<RemoteMsg<T>>,
//     {
//         match self {
//             AnySnd::Local(local) => local.send(msg),
//             AnySnd::Remote(remote) => remote.send(msg),
//         }
//     }

//     pub fn send(self, msg: T) -> Result<(), SendError<T>>
//     where
//         T: Into<RemoteMsg<T>>,
//     {
//         match self {
//             AnySnd::Local(local) => local.send(msg),
//             AnySnd::Remote(remote) => remote.send(msg),
//         }
//     }
// }

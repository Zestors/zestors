#![doc = include_str!("../../docs/protocol.md")]
#[allow(unused_imports)]
use crate::*;

mod boxed_msg;
mod default_impl;
mod protocol;
mod request;
pub use boxed_msg::*;
pub use default_impl::*;
pub use protocol::*;
pub use request::*;

pub use zestors_codegen::{protocol, Message};

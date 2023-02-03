#![doc = include_str!("../docs/core/supervision.md")]
#[allow(unused)]
use super::*;
#[doc(inline)]
pub use zestors_core::child::{
    Child, ChildGroup, ExitError, Group, IntoChild, IsGroup, NoGroup, ShutdownFut,
    ShutdownStream, *,
};

#![doc = include_str!("../../docs/supervision.md")]

#[allow(unused_imports)]
use crate::*;

mod child;
mod defines_pool;
mod shutdown;

pub use child::*;
pub use defines_pool::*;
pub use shutdown::*;

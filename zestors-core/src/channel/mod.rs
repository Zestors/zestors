#![doc = include_str!("../../docs/channel.md")]

use crate::*;

mod actor_id;
mod defines_channel;
mod dynamic;
mod halter_channel;
mod inbox_channel;
mod traits;

pub use actor_id::*;
pub use defines_channel::*;
pub use dynamic::*;
pub use halter_channel::*;
pub use inbox_channel::*;
pub use traits::*;
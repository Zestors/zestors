#![doc = include_str!("../docs/lib.md")]

pub mod distributed;
pub mod supervision;

// pub use zestors_core as core;

pub mod core {
    #![doc = include_str!("../../zestors-core/docs/lib.md")]
    pub use zestors_core::*;
}
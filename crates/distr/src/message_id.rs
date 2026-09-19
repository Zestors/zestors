use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A globally unique identifier for a message type, stable across nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Id(Uuid);

impl Id {
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Creates a [`Id`] from the raw 128 bits of a [`Uuid`].
    pub const fn from_u128(id: u128) -> Self {
        Self(Uuid::from_u128(id))
    }

    /// The underlying [`Uuid`].
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Gives a message type a stable, globally unique [`Id`].
///
/// Derive it with `#[derive(StableId)]` and `#[msg(id = "<uuid>")]`.
#[allow(non_upper_case_globals)]
pub trait StableId {
    const Id: Id;
}

pub use zestors_codegen::StableId;

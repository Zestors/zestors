use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Gives a message type a stable, globally unique [`MessageId`], which names it
/// on the wire.
///
/// Derive it with `#[derive(StableId)]` and `#[msg(id = "<uuid>")]`. Leave the
/// id out, and the compile error suggests a freshly generated one.
///
/// - **Never change the id** once nodes running different builds may talk to
///   each other: a node that doesn't know an id answers
///   [`RemoteError::UnknownMessage`](crate::RemoteError::UnknownMessage).
/// - **Never give two types the same id.** Registering both on one node panics
///   when the node is built.
#[allow(non_upper_case_globals)]
pub trait StableId {
    /// The stable, globally unique identifier for this message type.
    const Id: MessageId;
}

/// A globally unique identifier for a message type, stable across nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MessageId(Uuid);

impl MessageId {
    /// Creates a [`MessageId`] from a [`Uuid`].
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Creates a [`MessageId`] from the raw 128 bits of a [`Uuid`].
    pub const fn from_u128(id: u128) -> Self {
        Self(Uuid::from_u128(id))
    }

    /// The underlying [`Uuid`].
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for MessageId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<u128> for MessageId {
    fn from(id: u128) -> Self {
        Self(Uuid::from_u128(id))
    }
}

impl From<MessageId> for Uuid {
    fn from(id: MessageId) -> Self {
        id.0
    }
}

impl From<MessageId> for u128 {
    fn from(id: MessageId) -> Self {
        id.0.as_u128()
    }
}

pub use zestors_codegen::StableId;

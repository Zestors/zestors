use bytes::Bytes;
use serde::{Serialize, de::DeserializeOwned};
use std::error::Error;

/// Why a value couldn't be encoded.
#[derive(Debug, thiserror::Error)]
#[error("Failed to encode: {0}")]
pub struct EncodeError(#[source] Box<dyn Error + Send + Sync>);

impl EncodeError {
    pub fn new(error: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self(error.into())
    }
}

/// Why bytes couldn't be decoded into a value.
#[derive(Debug, thiserror::Error)]
#[error("Failed to decode: {0}")]
pub struct DecodeError(#[source] Box<dyn Error + Send + Sync>);

impl DecodeError {
    pub fn new(error: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self(error.into())
    }
}

/// A value that can be put on the wire.
///
/// Implemented for every type that implements [`serde::Serialize`], in the
/// [postcard](https://docs.rs/postcard) format. To use another format for a
/// type of your own, implement it by hand; that works for types that don't
/// implement `Serialize`, since the blanket impl would otherwise overlap. (For
/// a type that does, or one from another crate, wrap it in a newtype.)
///
/// Being remote is opt-in per message: [`Message`](zestors_interface::Message)
/// itself has no such bound.
pub trait Encode {
    fn encode(&self) -> Result<Bytes, EncodeError>;
}

/// A value that can be read from the wire, the counterpart of [`Encode`].
///
/// Implemented for every type that implements [`serde::de::DeserializeOwned`].
pub trait Decode: Sized {
    fn decode(bytes: Bytes) -> Result<Self, DecodeError>;
}

impl<T: Serialize> Encode for T {
    fn encode(&self) -> Result<Bytes, EncodeError> {
        postcard::to_allocvec(self)
            .map(Bytes::from)
            .map_err(EncodeError::new)
    }
}

impl<T: DeserializeOwned> Decode for T {
    fn decode(bytes: Bytes) -> Result<Self, DecodeError> {
        postcard::from_bytes(&bytes).map_err(DecodeError::new)
    }
}

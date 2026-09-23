use bytes::Bytes;
use serde::{Serialize, de::DeserializeOwned};
use std::error::Error;

/// Why a value couldn't be encoded.
#[derive(Debug, thiserror::Error)]
#[error("Failed to encode: {0}")]
pub struct EncodeError(#[source] Box<dyn Error + Send + Sync>);

impl EncodeError {
    /// Wraps the error that stopped the encoding.
    pub fn new(error: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self(error.into())
    }
}

/// Why bytes couldn't be decoded into a value.
#[derive(Debug, thiserror::Error)]
#[error("Failed to decode: {0}")]
pub struct DecodeError(#[source] Box<dyn Error + Send + Sync>);

impl DecodeError {
    /// Wraps the error that stopped the decoding.
    pub fn new(error: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self(error.into())
    }
}

/// A value that can be put on the wire.
///
/// Implemented for every type that implements [`serde::Serialize`], in the
/// [postcard](https://docs.rs/postcard) format.
///
/// To use another format, implement `Encode` and [`Decode`] by hand. That is
/// only possible for a type that doesn't implement `Serialize`, because it
/// would overlap with the serde implementation. For a type that does, or one
/// from another crate, wrap it in a newtype of your own.
///
/// A message sent to an actor on the same node is never encoded, so a broken
/// implementation only shows up once a message crosses the network.
pub trait Encode {
    /// Encodes the value into the bytes that are sent.
    fn encode(&self) -> Result<Bytes, EncodeError>;
}

/// A value that can be read from the wire, the counterpart of [`Encode`].
///
/// Implemented for every type that implements [`serde::de::DeserializeOwned`].
pub trait Decode: Sized {
    /// Decodes a value from the bytes that were received.
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

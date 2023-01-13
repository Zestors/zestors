use tokio::sync::OnceCell;

use crate::{self as zestors, *};
use std::{
    fmt::{self, Debug, Display, Formatter},
    mem::size_of,
};

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct Key {
    inner: Box<[u8]>,
}

impl Key {
    pub fn new(inner: Box<[u8]>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> &Box<[u8]> {
        &self.inner
    }
}

impl Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(debug_fn) = KEY_DEBUGGER.get() {
            if let Some(res) = debug_fn(self.clone(), f) {
                return res;
            }
        };

        f.debug_struct("Key").field("inner", &self.inner).finish()
    }
}

impl Display for Key {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        <Self as Debug>::fmt(&self, f)
    }
}

static KEY_DEBUGGER: OnceCell<fn(Key, &mut Formatter<'_>) -> Option<fmt::Result>> =
    OnceCell::const_new();

/// # Panics
/// - If the key was already set.
pub fn set_key<K: TryFrom<Key> + Debug>() {
    let function = |key, f: &mut Formatter<'_>| match K::try_from(key) {
        Ok(key) => Some(key.fmt(f)),
        Err(_) => None,
    };

    if let Err(_) = KEY_DEBUGGER.set(function) {
        panic!("Can't set key twice")
    }
}

//------------------------------------------------------------------------------------------------
//  Custom keys
//------------------------------------------------------------------------------------------------

macro_rules! impl_key_simple {
    ($($t:ty),*) => {$(
        impl From<$t> for Key {
            fn from(value: $t) -> Self {
                Key::new(Box::new(value.to_ne_bytes()))
            }
        }

        impl TryFrom<Key> for $t {
            type Error = ();

            fn try_from(value: Key) -> Result<Self, Self::Error> {
                let slice = value.inner();
                if slice.len() != size_of::<$t>() {
                    Err(())
                } else {
                    match (&**slice).try_into() {
                        Ok(integer) => Ok(<$t>::from_ne_bytes(integer)),
                        Err(_) => Err(()),
                    }
                }
            }
        }
    )*};
}

impl_key_simple!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128);

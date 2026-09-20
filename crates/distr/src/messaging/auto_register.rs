//! Registering every message type in the binary at once, see
//! [`Cluster::auto_register`].

use super::{Handlers, RemoteMessage};
use crate::ClusterConfig;
use std::marker::PhantomData;

/// A message type to register, collected from wherever it is derived.
#[doc(hidden)]
pub struct Registration {
    register: fn(&mut Handlers),
}

impl Registration {
    pub const fn new(register: fn(&mut Handlers)) -> Self {
        Self { register }
    }
}

/// Registers `M` if it is a [`RemoteMessage`], and does nothing if it isn't:
/// the derive of `StableId` can't tell, and a type may have it for another
/// reason. Picked between by autoref specialization: [`IfRemote`] is found
/// first, and only if `M` is a remote message, else [`IfNot`].
#[doc(hidden)]
pub struct Probe<M>(PhantomData<fn() -> M>);

impl<M> Probe<M> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<M> Default for Probe<M> {
    fn default() -> Self {
        Self::new()
    }
}

#[doc(hidden)]
pub trait IfRemote {
    fn register(&self, handlers: &mut Handlers);
}

impl<M: RemoteMessage> IfRemote for Probe<M> {
    fn register(&self, handlers: &mut Handlers) {
        handlers.insert::<M>();
    }
}

#[doc(hidden)]
pub trait IfNot {
    fn register(&self, _: &mut Handlers) {}
}

impl<M> IfNot for &Probe<M> {}

inventory::collect!(Registration);

impl ClusterConfig {
    /// [Registers](ClusterConfig::register) every message type in the binary
    /// that derives [`StableId`](crate::StableId), including those of the
    /// crates it depends on. Available with the `auto-register` feature.
    ///
    /// Types are collected where they are derived, so it makes no difference
    /// where or how often this is called. Only those that are a
    /// [`RemoteMessage`] are registered; a generic type has no one type to
    /// collect, and is registered with [`ClusterConfig::register`] like one
    /// that opts out with `#[msg(no_auto_register)]`.
    ///
    /// ```no_run
    /// # use zestors_distr::ClusterConfig;
    /// # fn example(config: ClusterConfig) -> ClusterConfig {
    /// config.auto_register()
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// If two of the collected types share a
    /// [`MessageId`](crate::MessageId), as
    /// [`ClusterConfig::register`] does. Collecting a type that was also
    /// registered by hand is not that, and only logs a warning.
    pub fn auto_register(mut self) -> Self {
        for registration in inventory::iter::<Registration> {
            (registration.register)(&mut self.handlers);
        }
        self
    }
}

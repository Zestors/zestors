use crate::*;
use std::sync::OnceLock;
use type_sets::{AsTypeSet, Members};

/// The process-wide registry of actors, mapping each [`Name`] to an
/// [`Address`]. Reach it with [`Registry::local`].
///
/// An actor is registered when its channel is created, and deregistered once
/// the last strong reference to it (a [`Child`], [`StrongAddress`] or
/// [`Inbox`]) is dropped — not when its task exits. An actor that has exited
/// but whose `StrongAddress` is still held, for example by a supervisor that
/// will restart it, stays registered.
#[derive(Debug)]
pub struct Registry {
    processes: papaya::HashMap<Name, Address>,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

impl Registry {
    /// Creates a new, empty process registry.
    fn new() -> Self {
        Self {
            processes: papaya::HashMap::new(),
        }
    }

    /// Returns every currently registered [`Address`], as a snapshot of the
    /// registry at the moment this is called.
    pub async fn fetch_addresses(&'static self) -> Vec<Address> {
        tokio::task::spawn_blocking(|| {
            self.processes
                .pin()
                .iter()
                .map(|(_name, addr)| addr.clone())
                .collect()
        })
        .await
        .expect("No cancel/panic")
    }

    /// Returns a reference to the global process registry singleton.
    pub fn local() -> &'static Self {
        REGISTRY.get_or_init(Self::new)
    }

    /// Registers a new process address.
    ///
    /// # Errors
    /// Returns a [`RegistryAddError`] if the [`Name`] is already registered.
    pub(crate) fn register<C: Context>(
        &self,
        address: Address<C>,
    ) -> Result<(), RegistryAddError<C>> {
        let map = self.processes.pin();

        if map
            .try_insert(address.name().clone(), address.clone().into_dyn())
            .is_err()
        {
            Err(RegistryAddError { address })
        } else {
            Ok(())
        }
    }

    /// Removes and returns the registered [`Address`] for a given [`Name`].
    pub(crate) fn remove(&self, name: &Name) -> Option<Address> {
        let guard = self.processes.guard();
        self.processes
            .remove_entry(name, &guard)
            .map(|(_name, addr)| addr.clone())
    }

    /// Fetches an untyped [`Address`] by its [`Name`], returning `None` if not registered.
    pub fn get(&self, name: &Name) -> Option<Address> {
        let guard = self.processes.guard();
        self.processes.get(name, &guard).cloned()
    }

    /// Fetches a strongly typed [`Address<C>`] by its [`Name`].
    ///
    /// # Errors
    /// Returns an error if
    /// - the process is not found
    /// - the address type downcast fails
    pub fn get_typed<I: Interface>(&self, name: &Name) -> Result<Address<I>, TypedRegistryError> {
        self.get(name)
            .ok_or_else(|| TypedRegistryError::NotFound(name.clone()))?
            .downcast::<I>()
            .map_err(|_| TypedRegistryError::TypeMismatch(name.clone()))
    }

    /// Fetches a dynamically typed [`Address<C>`] by its [`Name`].
    ///
    /// # Errors
    /// Returns an error if
    /// - the process is not found
    /// - the set of types does not match the registered address's type set
    pub fn get_dyn<S>(&self, name: &Name) -> Result<Address<Dyn<S>>, TypedRegistryError>
    where
        S: AsTypeSet + 'static + Members,
    {
        self.get(name)
            .ok_or_else(|| TypedRegistryError::NotFound(name.clone()))?
            .into_dyn_checked::<S>()
            .map_err(|_| TypedRegistryError::TypeMismatch(name.clone()))
    }

    /// Returns `true` if a process with the given [`Name`] is registered.
    pub fn contains(&self, name: &Name) -> bool {
        let guard = self.processes.guard();
        self.processes.contains_key(name, &guard)
    }
}

/// Error returned when registering a [`Name`] that already exists in the [`Registry`].
#[derive(thiserror::Error)]
#[error("Failed to add entry for name {}", .address.name())]
pub(crate) struct RegistryAddError<T: Context = Dyn> {
    address: Address<T>,
}

impl<T: Context> std::fmt::Debug for RegistryAddError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistryAddError")
            .field("entry", &self.address)
            .finish()
    }
}

/// Returned by [`Registry::get_typed`]/[`Registry::get_dyn`].
#[derive(thiserror::Error, Debug)]
pub enum TypedRegistryError {
    /// No process is registered under this [`Name`] at all.
    #[error("Address not found for name: {0}")]
    NotFound(Name),

    /// A process is registered under this [`Name`], but its concrete
    /// [`Interface`] doesn't match (for [`Registry::get_typed`]) or doesn't
    /// accept the requested message set (for [`Registry::get_dyn`]).
    #[error("Address found for name: {0} but type mismatch")]
    TypeMismatch(Name),
}

mod name;
pub use name::*;

use crate::_prelude::*;
use std::sync::OnceLock;
use type_sets::Members;

/// A thread-safe global registry mapping process identifiers ([`Pid`]) to their weak handles ([`Address`]).
///
/// Processes automatically register themselves in the registry upon creation
/// and deregister upon termination.
#[derive(Debug)]
pub struct Registry {
    processes: papaya::HashMap<Pid, Address>,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

impl Registry {
    /// Creates a new, empty process registry.
    fn new() -> Self {
        Self {
            processes: papaya::HashMap::new(),
        }
    }

    pub async fn fetch_addresses(&'static self) -> Vec<Address> {
        tokio::task::spawn_blocking(|| {
            self.processes
                .pin()
                .iter()
                .map(|(_pid, addr)| addr.clone())
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
    /// Returns a [`RegistryAddError`] if the [`Pid`] is already registered.
    pub(crate) fn register<C: Context>(
        &self,
        address: Address<C>,
    ) -> Result<(), RegistryAddError<C>> {
        let map = self.processes.pin();

        if map
            .try_insert(address.pid().clone(), address.clone().into_dyn())
            .is_err()
        {
            Err(RegistryAddError { address })
        } else {
            Ok(())
        }
    }

    /// Removes and returns the registered [`Address`] for a given [`Pid`].
    pub(crate) fn remove(&self, pid: &Pid) -> Option<Address> {
        let guard = self.processes.guard();
        self.processes
            .remove_entry(pid, &guard)
            .map(|(_pid, addr)| addr.clone())
    }

    /// Fetches an untyped [`Address`] by its [`Pid`], returning `None` if not registered.
    pub fn get(&self, pid: &Pid) -> Option<Address> {
        let guard = self.processes.guard();
        self.processes.get(pid, &guard).cloned()
    }

    /// Fetches a strongly typed [`Address<C>`] by its [`Pid`].
    ///
    /// # Errors
    /// Returns an error if
    /// - the process is not found
    /// - the address type downcast fails
    pub fn get_typed<I: Interface>(&self, pid: &Pid) -> Result<Address<I>, TypedRegistryError> {
        self.get(pid)
            .ok_or_else(|| TypedRegistryError::NotFound(pid.clone()))?
            .downcast::<I>()
            .map_err(|_| TypedRegistryError::TypeMismatch(pid.clone()))
    }

    /// Fetches a dynamically typed [`Address<C>`] by its [`Pid`].
    ///
    /// # Errors
    /// Returns an error if
    /// - the process is not found
    /// - the set of types does not match the registered address's type set
    pub fn get_dyn<S>(&self, pid: &Pid) -> Result<Address<Dyn<S>>, TypedRegistryError>
    where
        Dyn<S>: Context + Members,
    {
        self.get(pid)
            .ok_or_else(|| TypedRegistryError::NotFound(pid.clone()))?
            .into_dyn_checked::<S>()
            .map_err(|_| TypedRegistryError::TypeMismatch(pid.clone()))
    }

    /// Returns `true` if a process with the given [`Pid`] is registered.
    pub fn contains(&self, pid: &Pid) -> bool {
        let guard = self.processes.guard();
        self.processes.contains_key(pid, &guard)
    }
}

/// Error returned when registering a [`Pid`] that already exists in the [`Registry`].
#[derive(thiserror::Error)]
#[error("Failed to add entry for pid {}", .address.pid())]
pub struct RegistryAddError<T: Context = Dyn<()>> {
    address: Address<T>,
}

impl<T: Context> std::fmt::Debug for RegistryAddError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistryAddError")
            .field("entry", &self.address)
            .finish()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TypedRegistryError {
    #[error("Address not found for pid: {0}")]
    NotFound(Pid),

    #[error("Address found for pid: {0} but type mismatch")]
    TypeMismatch(Pid),
}

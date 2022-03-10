use std::{collections::HashMap, sync::RwLock};

use serde::{Serialize, Deserialize};

use crate::{actor::{Actor, ProcessId}, address::{Address, AnyAddress}};

use super::{
    local_node::LocalNode,
    pid::{AnyPid, NodeLocation, ProcessRef},
};

#[derive(Debug)]
pub struct Registry {
    by_id: RwLock<HashMap<ProcessId, AnyAddress>>,
    by_name: RwLock<HashMap<String, AnyAddress>>,
}

impl Registry {
    pub(crate) fn new() -> Self {
        Self {
            by_id: RwLock::new(HashMap::new()),
            by_name: RwLock::new(HashMap::new()),
        }
    }

    /// Register a process which is located on the local_node, this will then
    /// return a Pid, which can be sent to other processes
    pub(crate) fn register<A: Actor>(
        &self,
        address: &Address<A>,
        name: Option<String>,
        local_node: LocalNode,
    ) -> Result<(), RegistrationError> {
        // Aquire locks: first id, then name
        let mut by_id = self.by_id.write().unwrap();
        let mut by_name = self.by_name.write().unwrap();

        if by_id.contains_key(&address.process_id()) {
            return Err(RegistrationError::AlreadyRegistered)
        };

        if let Some(name) = &name {
            if by_name.contains_key(name) {
                return Err(RegistrationError::NameTaken)
            };
        };

        // Add the pid and name into the registry
        if let Some(name) = name {
            let old_name = by_name.insert(name, AnyAddress::new(address.clone()));
            assert!(old_name.is_none())
        }
        let old_id = by_id.insert(address.process_id(), AnyAddress::new(address.clone()));
        assert!(old_id.is_none());

        Ok(())
    }

    pub fn find_by_id<A: Actor>(
        &self,
        process_id: ProcessId,
    ) -> Result<Address<A>, RegistryGetError> {
        match self.by_id.read().unwrap().get(&process_id) {
            Some(any_address) => match any_address.downcast_ref::<A>() {
                Some(address) => Ok(address.clone()),
                None => Err(RegistryGetError::DownCastingFailed),
            },
            None => Err(RegistryGetError::IdNotRegistered),
        }
    }

    pub fn find_by_name<A: Actor>(&self, name: &str) -> Result<Address<A>, RegistryGetError> {
        match self.by_name.read().unwrap().get(name) {
            Some(any_address) => match any_address.downcast_ref::<A>() {
                Some(address) => Ok(address.clone()),
                None => Err(RegistryGetError::DownCastingFailed),
            },
            None => Err(RegistryGetError::IdNotRegistered),
        }
    }

    pub fn id_is_registered(&self, process_id: ProcessId) -> bool {
        self.by_id.read().unwrap().contains_key(&process_id)
    }

    pub fn name_is_registered(&self, name: &str) -> bool {
        self.by_name.read().unwrap().contains_key(name)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RegistryGetError {
    IdNotRegistered,
    DownCastingFailed,
}

#[derive(Debug)]
pub enum RegistrationError {
    NameTaken,
    AlreadyRegistered,
}

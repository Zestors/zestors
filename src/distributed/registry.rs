use std::{collections::HashMap, sync::RwLock};

use serde::{Serialize, Deserialize};

use crate::{actor::Actor, address::{Address, AnyAddress}};

use super::{
    local_node::LocalNode,
    pid::{AnyPid, NodeLocation, Pid},
    ProcessId,
};

#[derive(Debug)]
pub struct Registry {
    by_id: RwLock<HashMap<ProcessId, (AnyAddress, AnyPid)>>,
    by_name: RwLock<HashMap<String, (AnyAddress, AnyPid)>>,
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
    ) -> Result<Pid<A>, RegistrationError> {
        // Check if this address is already registered first
        if let Some(_) = address.is_registered() {
            return Err(RegistrationError::AlreadyRegistered);
        }

        // Generate a new temparary pid
        let pid = Pid::new(NodeLocation::Local(local_node));
        let process_id = pid.id();

        // Aquire locks: first id, then name
        let mut by_id = self.by_id.write().unwrap();
        let mut by_name = self.by_name.write().unwrap();
        assert!(!by_id.contains_key(&process_id));

        // Check if name is already registered
        if let Some(name) = &name {
            if by_name.contains_key(name) {
                return Err(RegistrationError::NameTaken);
            }
        }

        // Add the pid and name into the registry
        if let Some(name) = name {
            let old_name = by_name.insert(name, (AnyAddress::new(address.clone()), AnyPid::new(pid.clone())));
            assert!(old_name.is_none())
        }
        let old_id = by_id.insert(process_id, (AnyAddress::new(address.clone()), AnyPid::new(pid.clone())));
        assert!(old_id.is_none());

        // We can now drop the locks again
        drop(by_id);
        drop(by_name);

        // And insert the pid into the address
        let old_pid = address.replace_pid(pid.clone());
        assert!(old_pid.is_none());

        // Finally return the pid that was created
        Ok(pid)
    }

    pub(crate) fn pid_by_id<A: Actor>(
        &self,
        process_id: ProcessId,
    ) -> Result<Pid<A>, RegistryGetError> {
        match self.by_id.read().unwrap().get(&process_id) {
            Some(any) => match any.1.downcast_ref::<A>() {
                Some(pid) => Ok(pid.clone()),
                None => Err(RegistryGetError::DownCastingFailed),
            },
            None => Err(RegistryGetError::IdNotRegistered),
        }
    }

    pub(crate) fn pid_by_name<A: Actor>(&self, name: &str) -> Result<Pid<A>, RegistryGetError> {
        match self.by_name.read().unwrap().get(name) {
            Some(any) => match any.1.downcast_ref::<A>() {
                Some(pid) => Ok(pid.clone()),
                None => Err(RegistryGetError::DownCastingFailed),
            },
            None => Err(RegistryGetError::IdNotRegistered),
        }
    }

    // pub(crate) fn any_pid_by_id(
    //     &self,
    //     process_id: ProcessId,
    // ) -> Result<AnyPid, RegistryGetError> {
    //     match self.by_id.read().unwrap().get(&process_id) {
    //         Some(any) => Ok(any.1.clone()),
    //         None => Err(RegistryGetError::IdNotRegistered),
    //     }
    // }

    // pub(crate) fn any_pid_by_name(&self, name: &str) -> Result<AnyPid, RegistryGetError> {
    //     match self.by_name.read().unwrap().get(name) {
    //         Some(any) => Ok(any.1.clone()),
    //         None => Err(RegistryGetError::IdNotRegistered),
    //     }
    // }

    pub(crate) fn address_by_id<A: Actor>(
        &self,
        process_id: ProcessId,
    ) -> Result<Address<A>, RegistryGetError> {
        match self.by_id.read().unwrap().get(&process_id) {
            Some(any) => match any.0.downcast_ref::<A>() {
                Some(address) => Ok(address.clone()),
                None => Err(RegistryGetError::DownCastingFailed),
            },
            None => Err(RegistryGetError::IdNotRegistered),
        }
    }

    pub(crate) fn address_by_name<A: Actor>(&self, name: &str) -> Result<Address<A>, RegistryGetError> {
        match self.by_name.read().unwrap().get(name) {
            Some(any) => match any.0.downcast_ref::<A>() {
                Some(address) => Ok(address.clone()),
                None => Err(RegistryGetError::DownCastingFailed),
            },
            None => Err(RegistryGetError::IdNotRegistered),
        }
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

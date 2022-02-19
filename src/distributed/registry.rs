use std::{collections::HashMap, sync::RwLock};

use crate::actor::Actor;

use super::{
    pid::{AnyPid, Pid},
    ProcessId,
};

#[derive(Debug)]
pub(crate) struct Registry {
    by_id: RwLock<HashMap<ProcessId, AnyPid>>,
    by_name: RwLock<HashMap<String, AnyPid>>,
}

impl Registry {
    pub(crate) fn new() -> Self {
        Self {
            by_id: RwLock::new(HashMap::new()),
            by_name: RwLock::new(HashMap::new()),
        }
    }

    pub(crate) fn register<A: Actor>(
        &self,
        pid: Pid<A>,
        name: Option<String>,
    ) -> Result<ProcessId, RegistrationError<A>> {
        let process_id = pid.id();

        // Always aquire by_id first!
        let mut by_id = self.by_id.write().unwrap();
        let mut by_name = self.by_name.write().unwrap();

        if let Some(name) = &name {
            if by_name.contains_key(name) {
                return Err(RegistrationError::NameTaken(pid));
            }
        }

        if by_id.contains_key(&process_id) {
            Err(RegistrationError::AlreadyRegistered(pid))
        } else {
            if let Some(name) = name {
                by_name.insert(name, AnyPid::from_pid(pid.clone())).unwrap();
            }
            by_id.insert(process_id, AnyPid::from_pid(pid)).unwrap();
            Ok(process_id)
        }
    }

    pub(crate) fn get_by_id<A: Actor>(
        &self,
        process_id: ProcessId,
    ) -> Result<Pid<A>, RegistryGetError> {
        match self.by_id.read().unwrap().get(&process_id) {
            Some(any_pid) => match any_pid.downcast_ref::<A>() {
                Some(pid) => Ok(pid.clone()),
                None => Err(RegistryGetError::DownCastingFailed),
            },
            None => Err(RegistryGetError::IdNotRegistered),
        }
    }

    pub(crate) fn get_by_name<A: Actor>(
        &self,
        name: &str,
    ) -> Result<Pid<A>, RegistryGetError> {
        match self.by_name.read().unwrap().get(name) {
            Some(any_pid) => match any_pid.downcast_ref::<A>() {
                Some(pid) => Ok(pid.clone()),
                None => Err(RegistryGetError::DownCastingFailed),
            },
            None => Err(RegistryGetError::IdNotRegistered),
        }
    }
}

#[derive(Debug)]
pub enum RegistryGetError {
    IdNotRegistered,
    DownCastingFailed,
}

#[derive(Debug)]
pub enum RegistrationError<A: Actor> {
    NameTaken(Pid<A>),
    AlreadyRegistered(Pid<A>)
}
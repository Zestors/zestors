use crate::actor::{Actor, ProcessId};

use super::{
    challenge,
    pid::AnyProcessRef,
    registry::{Registry, RegistryGetError},
    remote_action::{RemoteAction, AbsPtr},
    NodeId,
};
use serde::{Deserialize, Serialize};

//------------------------------------------------------------------------------------------------
//  Message
//------------------------------------------------------------------------------------------------

/// The message type that is serialized, and sent over the websocket connection
#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum Msg {
    Challenge(Challenge),
    FindProcess(FindProcess),
    Action(Action),
}

impl Msg {
    pub(crate) fn serialize(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    pub(crate) fn deserialize(bytes: &[u8]) -> Result<Self, ()> {
        bincode::deserialize(bytes).map_err(|e| ())
    }
}

//------------------------------------------------------------------------------------------------
//  Challenge
//------------------------------------------------------------------------------------------------

/// Messages related to the challenge
#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum Challenge {
    Init(challenge::Challenge),
    Reply(challenge::ChallengeReply),
    Result(Result<(), challenge::ChallengeError>),
    ExchangeIds(NodeId),
}

//------------------------------------------------------------------------------------------------
//  ProcessRequest
//------------------------------------------------------------------------------------------------

/// Messages related to finding out where process is located
#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum FindProcess {
    RequestById(Id, ProcessId, FindProcessByIdFn),
    RequestByName(Id, String, FindProcessByNameFn),
    Reply(Id, Result<ProcessId, RegistryGetError>),
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct FindProcessByIdFn(AbsPtr);

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct FindProcessByNameFn(AbsPtr);

impl FindProcessByIdFn {
    pub(crate) fn new<A: Actor>() -> Self {
        Self(AbsPtr::new(Self::function::<A> as usize))
    }

    /// ### Safety:
    /// Serialize/Deserialize using same binary
    pub(crate) unsafe fn call(
        &self,
        registry: &Registry,
        process_id: ProcessId,
    ) -> Result<ProcessId, RegistryGetError> {
        let function: fn(&Registry, ProcessId) -> Result<ProcessId, RegistryGetError> =
            std::mem::transmute(self.0.get());
        function(registry, process_id)
    }

    fn function<A: Actor>(
        registry: &Registry,
        process_id: ProcessId,
    ) -> Result<ProcessId, RegistryGetError> {
        match registry.find_by_id::<A>(process_id) {
            Ok(_pid) => Ok(process_id),
            Err(e) => Err(e),
        }
    }
}

impl FindProcessByNameFn {
    pub(crate) fn new<A: Actor>() -> Self {
        Self(AbsPtr::new(Self::function::<A> as usize))
    }

    /// ### Safety:
    /// Serialize/Deserialize using same binary
    pub(crate) unsafe fn call(
        &self,
        registry: &Registry,
        name: &str,
    ) -> Result<ProcessId, RegistryGetError> {
        let function: fn(&Registry, &str) -> Result<ProcessId, RegistryGetError> =
            std::mem::transmute(self.0.get());
        function(registry, name)
    }

    fn function<A: Actor>(
        registry: &Registry,
        name: &str,
    ) -> Result<ProcessId, RegistryGetError> {
        match registry.find_by_name::<A>(name) {
            Ok(pid) => Ok(pid.process_id()),
            Err(e) => Err(e),
        }
    }
}

//------------------------------------------------------------------------------------------------
//  Message
//------------------------------------------------------------------------------------------------

/// Messages for exchanging messages between processes
#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum Action {
    /// The actual message that is sent
    Action(Id, RemoteAction),
    /// An acknowledgement that the message has been received, with information
    /// about whether the process is still alive
    Ack(Id, ProcessRegistered),
    /// A reply to the message that was sent. This reply is only sent if the action was a request,
    /// and not a message
    Reply(Id, Vec<u8>),
}

//------------------------------------------------------------------------------------------------
//  ProcessInfo
//------------------------------------------------------------------------------------------------

/// A process request, with information on whether the process is on that node, and whether
/// the actor is of the correct type
#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum ProcessInfo {
    /// The type_id was correct
    CorrectActorType,
    /// The type_id was incorrect
    IncorrectActorType,
    /// The process_id is not registered on this node
    NotRegistered,
}

//------------------------------------------------------------------------------------------------
//  ProcessRegistered
//------------------------------------------------------------------------------------------------

/// Information about whether the process is actually registered on this node
#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum ProcessRegistered {
    Yes,
    No,
}

//------------------------------------------------------------------------------------------------
//  ExchangeId
//------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct Id(u32);

#[derive(Debug)]
pub(crate) struct NextId(u32);

impl NextId {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn next(&mut self) -> Id {
        let id = self.0;
        self.0 += 1;
        Id(id)
    }
}

//------------------------------------------------------------------------------------------------
//  ActorTypeId
//------------------------------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ActorTypeId {
    id: u64,
}

impl ActorTypeId {
    pub(crate) fn new<A: Actor>() -> Self {
        Self {
            id: std::intrinsics::type_id::<A>(),
        }
    }

    pub(crate) fn is_of_type<A: Actor>(&self) -> bool {
        *self == Self::new::<A>()
    }

    pub(crate) unsafe fn from_raw(type_id: u64) -> Self {
        Self { id: type_id }
    }
}

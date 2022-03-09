use crate::actor::Actor;

use super::{
    challenge,
    pid::AnyPid,
    registry::{Registry, RegistryGetError},
    remote_action::RemoteAction,
    NodeId, ProcessId,
};
use serde::{Deserialize, Serialize};

//------------------------------------------------------------------------------------------------
//  Message
//------------------------------------------------------------------------------------------------

/// The message type that is serialized, and sent over the websocket connection
#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum Msg {
    Challenge(Challenge),
    Process(CheckForProcess),
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
pub(crate) enum CheckForProcess {
    Request(Id, ProcessId, CheckForProcessFn),
    Reply(Id, Result<(), RegistryGetError>),
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct CheckForProcessFn(usize);

impl CheckForProcessFn {
    pub fn new<A: Actor>() -> Self {
        Self(Self::function::<A> as usize)
    }

    /// Is safe serialization and deserialization happens using the same binary!
    pub unsafe fn call(
        &self,
        registry: &Registry,
        process_id: ProcessId,
    ) -> Result<(), RegistryGetError> {
        let function: fn(&Registry, process_id: ProcessId) -> Result<(), RegistryGetError> =
            std::mem::transmute(self.0);

        function(registry, process_id)
    }

    fn function<A: Actor>(
        registry: &Registry,
        process_id: ProcessId,
    ) -> Result<(), RegistryGetError> {
        registry.pid_by_id::<A>(process_id).map(|_| ())
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

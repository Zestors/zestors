use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    challenge::{Challenge, ChallengeError, ChallengeReply},
    NodeId,
};

/// This represents all message-types that can be sent over the websocket
/// connection between nodes
#[derive(Serialize, Deserialize, Debug)]
pub enum WsMsg {
    Challenge(Challenge),
    ChallengeReply(ChallengeReply),
    ChallengeResult(Result<(), ChallengeError>),
    NodeId(NodeId),
    SendAllNodes(Uuid, SocketAddr),
}

impl WsMsg {
    pub(crate) fn serialize(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    pub(crate) fn deserialize(bytes: &[u8]) -> Result<Self, ()> {
        bincode::deserialize(bytes).map_err(|e| ())
    }
}

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};
use sha2::{digest::Digest, Sha256};
use tokio::net::TcpStream;
use tokio_tungstenite::MaybeTlsStream;
use uuid::Uuid;
// use sha2::Digest;
use super::{
    local_node::{self, LocalNode},
    server::NodeSetupError,
    ws_message::WsMsg,
    ws_stream::WsStream,
    NodeId,
};

//------------------------------------------------------------------------------------------------
//  Challenge structs
//------------------------------------------------------------------------------------------------

/// A challenge, that can be sent to a node to verify that their build_id and
/// token are actually the same. This challenge is just a randomly generated
/// uuid, which must be hashed together with the build_id and token to create
/// a [ChallengeReply].
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub struct Challenge(Uuid);

/// A reply to a [Challenge], generated by hashing the [Challenge] together
/// with the build_id and
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct ChallengeReply {
    build_hash: Box<[u8]>,
    token_hash: Box<[u8]>,
}

impl ChallengeReply {
    fn new(local_node: &LocalNode, challenge: &Challenge) -> Self {
        let mut build_hash = Sha256::new();
        build_hash.update(challenge.0.as_bytes());
        build_hash.update(local_node.build_id().as_bytes());
        let build_hash: Box<[u8]> = build_hash.finalize().as_slice().into();

        let mut token_hash = Sha256::new();
        token_hash.update(challenge.0.as_bytes());
        token_hash.update(local_node.token().as_bytes());
        let token_hash: Box<[u8]> = token_hash.finalize().as_slice().into();

        Self {
            build_hash,
            token_hash,
        }
    }

    fn test(&self, challenge: &Challenge, local_node: &LocalNode) -> Result<(), ChallengeError> {
        let actual = ChallengeReply::new(local_node, challenge);

        match (
            self.build_hash == actual.build_hash,
            self.token_hash == actual.token_hash,
        ) {
            (true, true) => Ok(()),
            (true, false) => Err(ChallengeError::TokensDiffer),
            (false, true) => Err(ChallengeError::BuildsDiffer),
            (false, false) => Err(ChallengeError::TokenAndBuildDiffer),
        }
    }
}

//------------------------------------------------------------------------------------------------
//  Helper types
//------------------------------------------------------------------------------------------------

/// A [WsStream] that has been successfully challenged
#[derive(Debug)]
pub(crate) struct VerifiedStream(WsStream);

impl VerifiedStream {
    pub(crate) fn ws_stream(&mut self) -> &mut WsStream {
        &mut self.0
    }

    pub(crate) fn peer_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.0.peer_addr()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ChallengeError {
    TokensDiffer,
    BuildsDiffer,
    TokenAndBuildDiffer,
}

impl Challenge {
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

//------------------------------------------------------------------------------------------------
//  as_client
//------------------------------------------------------------------------------------------------

/// Challenge a websocket stream as a server. If succesful, returns a stream that is verified
/// to have been challenged.
pub(crate) async fn as_client(
    local_node: &LocalNode,
    mut stream: WsStream,
) -> Result<(NodeId, VerifiedStream), NodeSetupError> {
    // Wait for a challenge to be sent
    match stream.recv_next().await {
        Ok(WsMsg::Challenge(challenge)) => {
            // Send back a reply
            let response = ChallengeReply::new(&local_node, &challenge);
            stream.send(WsMsg::ChallengeReply(response)).await?;
        }
        Ok(msg) => return Err(NodeSetupError::Protocol(msg, "expected Challenge")),
        Err(e) => return Err(e.into()),
    }

    // Wait for the challenge result
    match stream.recv_next().await {
        Ok(WsMsg::ChallengeResult(result)) => {
            if let Err(e) = result {
                return Err(e.into());
            };
        }
        Ok(msg) => return Err(NodeSetupError::Protocol(msg, "expected ChallengeResult")),
        Err(e) => return Err(e.into()),
    }

    // Create and send a new challenge
    let challenge = Challenge::new();
    stream.send(WsMsg::Challenge(challenge)).await?;

    // Receive the challenge reply
    match stream.recv_next().await {
        Ok(WsMsg::ChallengeReply(reply)) => {
            match reply.test(&challenge, &local_node) {
                Err(e) => {
                    // Send back a failure result
                    stream.send(WsMsg::ChallengeResult(Err(e.clone()))).await?;
                    return Err(e.into());
                }
                Ok(_) => {
                    // Send back a success result
                    stream.send(WsMsg::ChallengeResult(Ok(()))).await?;
                }
            }
        }
        Ok(msg) => return Err(NodeSetupError::Protocol(msg, "expected ChallengeReply")),
        Err(e) => return Err(e.into()),
    }

    // Then exchange node ids
    let node_id = exchange_node_ids(local_node, &mut stream).await?;

    // Challenge is now complete, we can return the challenged stream
    Ok((node_id, VerifiedStream(stream)))
}

//------------------------------------------------------------------------------------------------
//  as_server
//------------------------------------------------------------------------------------------------

pub(crate) async fn as_server(
    local_node: &LocalNode,
    mut stream: WsStream,
) -> Result<(NodeId, VerifiedStream), NodeSetupError> {
    // Create and send a new challenge
    let challenge = Challenge::new();
    stream.send(WsMsg::Challenge(challenge)).await?;

    // Receive the challenge reply
    match stream.recv_next().await {
        Ok(WsMsg::ChallengeReply(reply)) => {
            match reply.test(&challenge, &local_node) {
                Err(e) => {
                    // Send back a failure result
                    stream.send(WsMsg::ChallengeResult(Err(e.clone()))).await?;
                    return Err(e.into());
                }
                Ok(_) => {
                    // Send back a success result
                    stream.send(WsMsg::ChallengeResult(Ok(()))).await?;
                }
            }
        }
        Ok(msg) => return Err(NodeSetupError::Protocol(msg, "expected ChallengeReply")),
        Err(e) => return Err(e.into()),
    }

    // Wait for a challenge to be sent
    match stream.recv_next().await {
        Ok(WsMsg::Challenge(challenge)) => {
            // Send back a reply
            let response = ChallengeReply::new(&local_node, &challenge);
            stream.send(WsMsg::ChallengeReply(response)).await?;
        }
        Ok(msg) => return Err(NodeSetupError::Protocol(msg, "expected Challenge")),
        Err(e) => return Err(e.into()),
    }

    // Wait for the challenge result
    match stream.recv_next().await {
        Ok(WsMsg::ChallengeResult(result)) => {
            if let Err(e) = result {
                return Err(e.into());
            };
        }
        Ok(msg) => return Err(NodeSetupError::Protocol(msg, "expected ChallengeResult")),
        Err(e) => return Err(e.into()),
    }

    // Then exchange node ids
    let node_id = exchange_node_ids(local_node, &mut stream).await?;

    // Challenge is now complete, we can return the challenged stream
    Ok((node_id, VerifiedStream(stream)))
}

async fn exchange_node_ids(local_node: &LocalNode, stream: &mut WsStream) -> Result<NodeId, NodeSetupError> {
    stream.send(WsMsg::NodeId(local_node.node_id())).await?;
    
    match stream.recv_next().await? {
        WsMsg::NodeId(node_id) => Ok(node_id),
        msg => Err(NodeSetupError::Protocol(msg, "expected NodeId"))
    }
}
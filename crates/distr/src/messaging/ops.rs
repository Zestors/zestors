//! Operations on an actor, as opposed to messages for it: signalling it,
//! checking that it is alive, and asking about its state. They are what
//! [`RemoteActorOps`](super::RemoteActorOps) sends.
//!
//! They are messages like any other on the wire, with ids of their own, and
//! every node handles them without them being registered. Unlike messages,
//! they are for the actor's channel and not its mailbox: they don't wait
//! behind the messages queued for the actor, just as signals don't locally.

use super::{
    RemoteError,
    handler::{Builtin, Handler, Operation},
};
use crate::{Id, StableId};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::{marker::PhantomData, sync::Arc};
use zestors_interface::Message;
use zestors_runtime::{Address, ChannelSnapshot, Signal, prelude::*};

use super::Shared;

/// Sends a [`Signal`] to the actor. Answered with whether it was accepted:
/// `false` if the actor was already exiting or dead.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = bool, id = "b5c3f7a0-5f0e-4b0f-9f7e-2f6c1f0a0001")]
#[zestors(interface_path = "zestors_interface", distr_path = "crate")]
pub(super) struct SignalOp(pub(super) Signal);

/// Waits for the actor to process a signal. Answered once it has.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = (), id = "b5c3f7a0-5f0e-4b0f-9f7e-2f6c1f0a0002")]
#[zestors(interface_path = "zestors_interface", distr_path = "crate")]
pub(super) struct PingOp;

/// Asks about the state of the actor.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = RemoteInfo, id = "b5c3f7a0-5f0e-4b0f-9f7e-2f6c1f0a0003")]
#[zestors(interface_path = "zestors_interface", distr_path = "crate")]
pub(super) struct InfoOp;

/// The state of an actor on another node, at one instant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteInfo {
    /// The actor's status, queue lengths and spawn and exit history. The
    /// timestamps are the clock of the node the actor is on.
    pub snapshot: ChannelSnapshot,
    /// Whether the actor's mailbox is full, so that sending to it waits.
    pub reached_backpressure: bool,
    /// The ids of the message types that the actor accepts and that its node
    /// has registered with [`Remote::register`](super::Remote::register). `None`
    /// for an actor on this node, which can't tell.
    pub accepts: Option<Vec<Id>>,
}

impl Operation for SignalOp {
    fn run(
        self,
        address: Address,
        _: Arc<Shared>,
    ) -> super::handler::BoxFuture<Result<bool, RemoteError>> {
        Box::pin(async move { Ok(address.signal(self.0)) })
    }
}

impl Operation for PingOp {
    fn run(
        self,
        address: Address,
        _: Arc<Shared>,
    ) -> super::handler::BoxFuture<Result<(), RemoteError>> {
        Box::pin(async move { address.ping().await.map_err(|_| RemoteError::NoReply) })
    }
}

impl Operation for InfoOp {
    fn run(
        self,
        address: Address,
        shared: Arc<Shared>,
    ) -> super::handler::BoxFuture<Result<RemoteInfo, RemoteError>> {
        Box::pin(async move {
            let mut accepts: Vec<Id> = shared
                .handlers
                .iter()
                .filter(|handler| handler.accepts(&address))
                .map(|handler| *handler.key())
                .collect();
            accepts.sort();
            Ok(RemoteInfo {
                snapshot: address.snapshot(),
                reached_backpressure: address.reached_backpressure(),
                accepts: Some(accepts),
            })
        })
    }
}

/// Makes a node handle the operations.
pub(super) fn register(handlers: &DashMap<Id, Arc<dyn Handler>>) {
    handlers.insert(SignalOp::Id, Arc::new(Builtin::<SignalOp>(PhantomData)));
    handlers.insert(PingOp::Id, Arc::new(Builtin::<PingOp>(PhantomData)));
    handlers.insert(InfoOp::Id, Arc::new(Builtin::<InfoOp>(PhantomData)));
}

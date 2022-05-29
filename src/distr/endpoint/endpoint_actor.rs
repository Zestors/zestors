use crate as zestors;
use crate::{core::*, distr::*, Action};
use async_trait::async_trait;
use derive_more::{Display, Error};
use futures::Stream;
use log::{info, warn};
use quinn::VarInt;
use std::task::Poll;
use uuid::Uuid;
use zestors_codegen::zestors;
use zestors_codegen::Addr;

//------------------------------------------------------------------------------------------------
//  EndpointActor
//------------------------------------------------------------------------------------------------

/// The actor that does the following things:
/// - Checks for incoming quinn connections, and spawns new nodes from them.
/// - Listens for incoming messages, and handles them appropriately. (ie shutdown)
/// - Checks when a node exits, and handles this exit appropriately.
#[derive(Addr, Debug)]
#[addr(pub EndpointAddr)]
pub struct EndpointActor {
    // The cloneable system, which contains all `Node`s registered on this system
    endpoint: Endpoint,
    // Cloneable endpoint.
    quinn_endpoint: quinn::Endpoint,
    // Uncloneable incoming connections.
    incoming: quinn::Incoming,
    // The `SystemNodes` registered on this system.
    nodes: NodeChildren,
}

/// Implement the Actor trait
#[async_trait]
impl Actor for EndpointActor {
    type Init = (quinn::Endpoint, quinn::Incoming, NodeId);
    type Error = anyhow::Error;
    type Halt = ();
    type Exit = (Self, Event<Self>);

    /// Initializing the SystemActor should never fail!. Anything that can fail should be done
    /// before spawning.
    async fn initialize(
        (quinn_endpoint, incoming, node_id): Self::Init,
        addr: Self::Addr<Local>,
    ) -> InitFlow<Self> {
        let endpoint = Endpoint::new(addr, quinn_endpoint.clone(), node_id);
        InitFlow::Init(Self {
            endpoint,
            quinn_endpoint,
            incoming,
            nodes: NodeChildren::new(),
        })
    }

    fn handle_event(self, signal: Event<Self>, _state: &mut State<Self>) -> EventFlow<Self> {
        self.quinn_endpoint.close(VarInt::from_u32(0), &[]);
        info!(
            "[E{}] exiting with Signal({:?})",
            self.endpoint.id(),
            signal
        );
        EventFlow::Exit((self, signal))
    }
}

/// Here we set up the scheduler for EndpointActor. It listens for incoming connections, and also
impl Stream for EndpointActor {
    type Item = Action<Self>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        {
            if let Poll::Ready(connecting) = self.incoming.poll_next_unpin(cx) {
                Poll::Ready(Some(
                    Action!(EndpointActor::handle_incoming_quic_conn, connecting).0,
                ))
            } else if let Poll::Ready(node_exit) = self.nodes.poll_next_unpin(cx) {
                Poll::Ready(Some(Action!(Self::handle_system_node_exit, node_exit).0))
            } else {
                Poll::Pending
            }
        }
    }
}

/// All messages that can be sent to this Endpoint.
#[zestors(impl EndpointAddr)]
impl EndpointActor {
    /// Get the `System` that this actor stores.
    #[As(pub(crate) get_endpoint)]
    fn handle_get_system(&mut self, _msg: (), snd: Snd<Endpoint>) -> FlowResult<Self> {
        let _ = snd.send(self.endpoint.clone());
        Ok(Flow::Cont)
    }

    /// Attempt to register a new outgoing node.
    #[As(pub(crate) register_new_outgoing_node)]
    async fn handle_registering_new_outgoing_node(
        &mut self,
        system_node: NodeChild,
        snd: Snd<Result<Node, NodeRegistrationError>>,
    ) -> FlowResult<Self> {
        let node = system_node.node().clone();
        let system_id = system_node.id();

        if self.endpoint.id() == system_node.id() {
            let _ = snd.send(Err(NodeRegistrationError::EndpointIdIsOwnId));
            return Ok(Flow::Cont);
        }

        match (
            self.endpoint.node_store().store_node(node.clone()),
            self.nodes.store_node(system_node),
        ) {
            (Ok(()), Ok(())) => {
                // Both insertions were ok -> establish the connection
                info!("[E{}] Initiated connection to  E{}", self.endpoint.id(), system_id);
                let _ = snd.send(Ok(node));
            }
            (Ok(()), Err(mut system_node)) => {
                // One insertion failed -> remove this and abort the node
                warn!("Outgoing SystemId already connected: {:?}", system_id);
                let _node = self.endpoint.node_store().remove_node(&system_id).unwrap();
                system_node.disconnect();
                let _ = snd.send(Err(NodeRegistrationError::AlreadyRegistered(
                    system_node.id(),
                )));
                let _s = system_node.await.unwrap();
            }
            (Err(_node), Ok(())) => {
                // One insertion failed -> remove this and abort the node
                warn!("Outgoing SystemId already connected: {:?}", system_id);
                let mut system_node = self.nodes.remove_node(&system_id).unwrap();
                system_node.disconnect();
                let _ = snd.send(Err(NodeRegistrationError::AlreadyRegistered(
                    system_node.id(),
                )));
                let _ = system_node.await.unwrap();
            }
            (Err(_node), Err(mut system_node)) => {
                // One insertion failed -> remove this and abort the node
                warn!("Outgoing SystemId already connected: {:?}", system_id);
                system_node.disconnect();
                let _ = snd.send(Err(NodeRegistrationError::AlreadyRegistered(
                    system_node.id(),
                )));
                let _ = system_node.await.unwrap();
            }
        }
        Ok(Flow::Cont)
    }

    /// Attempt to register a new incoming node.
    #[As(pub(crate) register_new_incoming_node)]
    async fn handle_registering_new_incoming_node(
        &mut self,
        system_node: NodeChild,
    ) -> FlowResult<Self> {
        let node = system_node.node().clone();
        let system_id = system_node.id();

        if self.endpoint.id() == system_node.id() {
            warn!("Connection incoming with own EndpointId");
            return Ok(Flow::Cont);
        }

        match (
            self.endpoint.node_store().store_node(node),
            self.nodes.store_node(system_node),
        ) {
            (Ok(()), Ok(())) => {
                // Both insertions were ok -> establish the connection
                info!("[E{}] Accepted connection from  E{}", self.endpoint.id(), system_id);
            }
            (Ok(()), Err(mut system_node)) => {
                // One insertion failed -> remove this and abort the node
                warn!("Incoming SystemId already connected: {:?}", system_id);
                let _node = self.endpoint.node_store().remove_node(&system_id).unwrap();
                system_node.disconnect();
                let _s = system_node.await.unwrap();
            }
            (Err(_node), Ok(())) => {
                // One insertion failed -> remove this and abort the node
                warn!("Incoming SystemId already connected: {:?}", system_id);
                let mut system_node = self.nodes.remove_node(&system_id).unwrap();
                system_node.disconnect();
                let _ = system_node.await.unwrap();
            }
            (Err(_node), Err(mut system_node)) => {
                // One insertion failed -> remove this and abort the node
                warn!("Incoming SystemId already connected: {:?}", system_id);
                system_node.disconnect();
                let _ = system_node.await.unwrap();
            }
        }
        Ok(Flow::Cont)
    }
}

/// An error returned when attempting to register a new node.
#[derive(dm::Error, dm::Display, Debug)]
#[display(fmt = "Registration of node has failed: {}")]
pub enum NodeRegistrationError {
    /// This node was already registered on this endpoint.
    #[display(fmt = "Endpoint with id {} was already registered.", _0)]
    AlreadyRegistered(#[error(not(source))] NodeId),

    /// The endpoint id is the same as it's own id.
    #[display(fmt = "Tried to register endpoint on itself!")]
    EndpointIdIsOwnId,
}

//------------------------------------------------------------------------------------------------
//  Action handling
//------------------------------------------------------------------------------------------------

// Spawns a seperate task that handles the connection, so that the actor can move on.
fn handle_incoming_quic_conn(connecting: quinn::Connecting, system: Endpoint) {
    tokio::task::spawn(async move {
        match NodeActor::spawn(connecting, system.clone()).await {
            Ok(system_node) => {
                let _ = system.addr().register_new_incoming_node(system_node);
            }
            Err(e) => {
                warn!(
                    "Initialization of new incoming system connection has failed: {}",
                    e
                );
            }
        }
    });
}

impl EndpointActor {
    pub(crate) async fn spawn(
        endpoint: quinn::Endpoint,
        incoming: quinn::Incoming,
        node_id: NodeId,
    ) -> (Child<Self>, Endpoint) {
        let (child, addr): (Child<_>, EndpointAddr) =
            spawn_actor::<EndpointActor>((endpoint, incoming, node_id));

        // Then retrieve the Endpoint from this actor
        let endpoint: Endpoint = addr
            .get_endpoint(())
            .into_rcv()
            .await
            .expect("EndpointActor should never die here");

        (child, endpoint)
    }

    async fn handle_incoming_quic_conn(
        &mut self,
        connecting: Option<quinn::Connecting>,
    ) -> FlowResult<Self> {
        match connecting {
            Some(incoming) => {
                handle_incoming_quic_conn(incoming, self.endpoint.clone());
                Ok(Flow::Cont)
            }
            None => Err(anyhow::anyhow!("Endpoint for system has been closed")),
        }
    }

    async fn handle_system_node_exit(
        &mut self,
        incoming: Option<(NodeId, Result<NodeActorExit, ExitError>, NodeChild)>,
    ) -> FlowResult<Self> {
        match incoming {
            Some((id, exit, _node)) => match exit {
                Ok(_exit) => {
                    self.endpoint
                        .node_store()
                        .remove_node(&id)
                        .expect("Node should still be registered");
                    info!("[E{}] Node {} exit handled by endpoint.", self.endpoint.id(), id)
                }
                Err(e) => {
                    panic!("Node {} exited with error {:?}", id, e);
                }
            },
            None => todo!(),
        }
        Ok(Flow::Cont)
    }
}

use self::msg::RegisterNode;

use super::*;
use crate::*;
use std::{io, net::SocketAddr, sync::Arc};
use tokio::net::{TcpListener, TcpStream};
use zestors::{error::RecvError, process::Inbox};
use zestors_codegen::protocol;
use zestors_core as zestors;

#[protocol]
#[derive(Debug)]
pub(super) enum SystemMsg {
    RegisterChild(msg::RegisterNode),
}

pub(super) struct System {
    inbox: Inbox<SystemMsg>,
    listener: TcpListener,
    system_ref: SystemRef,
    children: Vec<Child<NodeExit>>,
}

impl System {
    pub fn new(inbox: Inbox<SystemMsg>, listener: TcpListener, system_ref: SystemRef) -> Self {
        System {
            inbox,
            listener,
            system_ref,
            children: Vec::new(),
        }
    }

    pub async fn run(mut self) -> SystemExit {
        loop {
            let res = tokio::select! {
                msg = self.inbox.recv() =>  {
                    self.handle_recv(msg).await
                }

                conn = self.listener.accept() => {
                    self.handle_conn(conn).await
                }
            };

            if let Err(exit) = res {
                break (exit);
            }
        }
    }

    async fn handle_conn(
        &mut self,
        incoming: Result<(TcpStream, SocketAddr), io::Error>,
    ) -> Result<(), SystemExit> {
        if let Ok((stream, socket)) = incoming {
            let address = self.inbox.get_address();
            let system_ref = self.system_ref.clone();

            tokio::task::spawn(async move {
                // If the node can be spawned
                if let Ok((child, node_ref)) = NodeRef::spawn(stream, socket, system_ref).await {
                    // Then attempt to register it on the node.
                    // Any errors here can be discarded.
                    let _ = address.send(msg::RegisterNode(child, node_ref)).await;
                }
            });
        }
        Ok(())
    }

    async fn handle_recv(&mut self, msg: Result<SystemMsg, RecvError>) -> Result<(), SystemExit> {
        match msg {
            Ok(msg) => self.handle_msg(msg).await,
            Err(e) => match e {
                RecvError::Halted => {
                    self.inbox.close();
                    Ok(())
                }
                RecvError::ClosedAndEmpty => Err(SystemExit::Halted),
            },
        }
    }

    async fn handle_msg(&mut self, msg: SystemMsg) -> Result<(), SystemExit> {
        match msg {
            SystemMsg::RegisterChild((RegisterNode(child, node_ref), tx)) => {
                match self.system_ref.shared.register_node(node_ref).await {
                    Ok(()) => {
                        self.children.push(child);
                        let _ = tx.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                    }
                }
                Ok(())
            }
        }
    }
}

pub(super) mod msg {
    use crate::{ConnectError, NodeExit, NodeRef};
    use zestors::process::Child;
    use zestors_codegen::Message;
    use zestors_core as zestors;
    use zestors_request::Rx;

    #[derive(Message, Debug)]
    #[msg(Rx<Result<(), ConnectError>>)]
    pub(crate) struct RegisterNode(pub Child<NodeExit>, pub NodeRef);
}

use crate::*;
use futures::StreamExt;
use quinn::{Connecting, Endpoint, Incoming};
use std::net::SocketAddr;
use zestors::{error::RecvError, process::spawn};
use zestors_codegen::{protocol, Message};
use zestors_core::{
    self as zestors,
    config::Config,
    process::Inbox,
    process::{Address, Child},
};

//------------------------------------------------------------------------------------------------
//  SystemRef
//------------------------------------------------------------------------------------------------

#[derive(Clone, Message, Debug)]
pub struct SystemRef {
    endpoint: Endpoint,
    address: Address<SystemMsg>,
}

impl SystemRef {
    /// Spawn a new local system.
    /// 
    /// This method should be called a single time to start the local system. Once this is spawned,
    /// this system can connect to multiple nodes.
    #[allow(unreachable_patterns)]
    pub fn spawn(addr: SocketAddr, config: Config) -> (Child<SystemExit>, Self) {
        let (endpoint, incoming) = Endpoint::server(config::insecure_server(), addr).unwrap();

        let (child, address) = spawn(config, |mut inbox: Inbox<SystemMsg>| async move {
            match inbox.recv().await.unwrap() {
                SystemMsg::Init(system_ref) => {
                    System {
                        incoming,
                        system_ref,
                        inbox,
                    }
                    .run()
                    .await
                }
                _ => unreachable!("First message is always the SystemRef"),
            }
        });

        let system_ref = SystemRef {
            endpoint,
            address: address,
        };
        system_ref.address.send_now(system_ref.clone()).unwrap();

        (child.into_dyn(), system_ref)
    }

    /// Connect the system to a remote system.
    /// 
    /// If successful, this will return the NodeRef to this system that can be used for
    /// communication.
    pub async fn connect(&self, addr: SocketAddr) -> () {
        match self.endpoint.connect(addr, "test") {
            Ok(conn) => match conn.await {
                Ok(conn) => todo!(),
                Err(e) => todo!(),
            },
            Err(e) => todo!(),
        }
    }
}

//------------------------------------------------------------------------------------------------
//  System
//------------------------------------------------------------------------------------------------

struct System {
    incoming: Incoming,
    system_ref: SystemRef,
    inbox: Inbox<SystemMsg>,
}

impl System {
    async fn run(mut self) -> SystemExit {
        loop {
            let res = tokio::select! {
                incoming = self.incoming.next() => {
                    self.handle_incoming(incoming).await
                }

                msg = self.inbox.recv() =>  {
                    self.handle_msg(msg).await
                }
            };

            if let Some(exit) = res {
                break (exit);
            }
        }
    }

    async fn handle_incoming(&mut self, incoming: Option<Connecting>) -> Option<SystemExit> {
        match incoming {
            Some(connecting) => match connecting.await {
                Ok(new_conn) => {
                    let conn = new_conn.connection;
                    let bi_streams = new_conn.bi_streams;
                    let uni_streams = new_conn.uni_streams;
                    None
                }
                Err(e) => None,
            },
            None => Some(SystemExit::Closed),
        }
    }

    async fn handle_msg(&mut self, msg: Result<SystemMsg, RecvError>) -> Option<SystemExit> {
        match msg {
            Ok(msg) => match msg {
                SystemMsg::Init(_) => {
                    unreachable!("Can only be sent once on initialization")
                }
            },
            Err(e) => match e {
                RecvError::Halted => Some(SystemExit::Halted),
                RecvError::ClosedAndEmpty => Some(SystemExit::Halted),
            },
        }
    }
}

//------------------------------------------------------------------------------------------------
//  SystemMsg
//------------------------------------------------------------------------------------------------

#[protocol]
#[derive(Debug)]
enum SystemMsg {
    Init(SystemRef),
}

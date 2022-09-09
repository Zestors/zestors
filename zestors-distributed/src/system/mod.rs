use self::{actor::*, shared::*};
use crate::*;
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io,
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
use zestors_codegen::Message;
use zestors_core::{
    self as zestors,
    config::Config,
    process::Inbox,
    process::{spawn, Address, Child},
};

mod actor;
mod shared;

#[derive(Clone, Message, Debug)]
pub struct SystemRef {
    address: Address<SystemMsg>,
    shared: Arc<SharedSystem>,
}

impl SystemRef {
    /// Spawn a new local system.
    ///
    /// This method should be called a single time to start the local system. Once this is spawned,
    /// this system can connect to multiple nodes.
    #[allow(unreachable_patterns)]
    pub async fn spawn(addr: SocketAddr, config: Config) -> (Child<SystemExit>, Self) {
        let listener = TcpListener::bind(addr).await.unwrap();
        let (tx, rx) = oneshot::channel();

        let (child, address) = spawn(config, |inbox: Inbox<SystemMsg>| async move {
            let inner_system = rx.await.unwrap();
            System::new(inbox, listener, inner_system).run().await
        });

        let inner_system = Arc::new(SharedSystem::new());
        tx.send(inner_system.clone()).unwrap();

        let system_ref = SystemRef {
            address,
            shared: inner_system,
        };
        (child.into_dyn(), system_ref)
    }

    /// Connect the system to a remote system.
    ///
    /// If successful, this will return the NodeRef to this system that can be used for
    /// communication.
    pub async fn connect(&self, socket: &SocketAddr) -> Result<(), ConnectError> {
        if self.shared.is_connected(socket).await {
            Err(ConnectError::AlreadyConnected)
        } else {
            match TcpStream::connect(socket).await {
                Ok(stream) => {
                    todo!();
                }
                Err(e) => Err(ConnectError::CouldntConnect(e)),
            }
        }
    }
}

pub enum ConnectError {
    AlreadyConnected,
    CouldntConnect(io::Error),
    Io(io::Error),
}

impl From<io::Error> for ConnectError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub enum SystemExit {
    Closed,
    Halted,
    IoError(io::Error)
}
use self::{actor::*, shared::*};
use crate::*;
use std::{net::SocketAddr, sync::Arc};
use tokio::{io, net::TcpListener, sync::oneshot};
use zestors_codegen::Message;
use zestors_core::{
    self as zestors,
    config::Config,
    process::Inbox,
    process::{spawn, Address, Child},
};
use zestors_request::IntoRecv;

mod actor;
mod shared;

#[derive(Clone, Message, Debug)]
pub struct SystemRef {
    address: Address<SystemMsg>,
    pub(super) shared: Arc<SystemShared>,
}

impl SystemRef {
    /// Spawn a new local system.
    ///
    /// This method should be called a single time to start the local system. Once this is spawned,
    /// this system can connect to multiple nodes.
    #[allow(unreachable_patterns)]
    pub async fn spawn(
        addr: SocketAddr,
        id: u64,
    ) -> Result<(Child<SystemExit>, SystemRef), SystemSpawnError> {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| SystemSpawnError::BindError(e))?;
        let (tx, rx) = oneshot::channel();

        let (child, address) = spawn(Config::default(), |inbox: Inbox<SystemMsg>| async move {
            let inner_system = rx.await.unwrap();
            System::new(inbox, listener, inner_system).run().await
        });

        let system_ref = SystemRef {
            address,
            shared: Arc::new(SystemShared::new(id)),
        };
        tx.send(system_ref.clone()).unwrap();

        Ok((child.into_dyn(), system_ref))
    }

    /// Connect the system to a remote system.
    ///
    /// If successful, this will return the NodeRef to this system that can be used for
    /// communication.
    pub async fn connect(&self, socket: SocketAddr) -> Result<NodeRef, ConnectError> {
        // First, we spawn the node.
        // This will exchange the ids and start listening.
        let (child, node_ref) = NodeRef::spawn_connect(socket, self.clone()).await?;

        // If succesful, we can send the child to our system to be supervised.
        let reply = self
            .address
            .send(msg::RegisterNode(child, node_ref.clone()))
            .into_recv()
            .await
            .map_err(|_| ConnectError::SystemDied)?;

        match reply {
            Ok(()) => Ok(node_ref),
            Err(e) => Err(e),
        }
    }

    pub fn id(&self) -> u64 {
        self.shared.id()
    }
}

#[derive(Debug)]
pub enum ConnectError {
    AlreadyConnected,
    TcpSetupFailed(io::Error),
    Io(io::Error),
    SystemDied,
    ConnectedToSelf,
}

#[derive(Debug)]
pub enum SystemSpawnError {
    BindError(io::Error),
}

impl From<io::Error> for ConnectError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

pub enum SystemExit {
    Closed,
    Halted,
    IoError(io::Error),
}

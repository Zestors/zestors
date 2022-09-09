use std::{collections::HashMap, net::SocketAddr};

use crate::*;
use tokio::{net::TcpStream, sync::Mutex};

#[derive(Debug)]
pub(super) struct SharedSystem {
    connections: Mutex<HashMap<SocketAddr, TcpStream>>,
}

impl SharedSystem {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
        }
    }

    pub async fn add_connection(&self, stream: TcpStream) -> Result<(), ConnectError> {
        let socket = stream.peer_addr()?;
        let mut connections = self.connections.lock().await;

        if connections.contains_key(&socket) {
            Err(ConnectError::AlreadyConnected)
        } else {
            connections.insert(socket, stream);
            todo!()
        }
    }

    pub async fn is_connected(&self, socket: &SocketAddr) -> bool {
        self.connections.lock().await.contains_key(socket)
    }
}

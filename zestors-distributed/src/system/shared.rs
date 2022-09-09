use std::collections::HashMap;

use crate::*;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug)]
pub(crate) struct SystemShared {
    session_id: Uuid,
    id: u64,
    connections: Mutex<HashMap<NodeId, NodeRef>>,
}

impl SystemShared {
    pub fn new(id: u64) -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            session_id: Uuid::new_v4(),
            id,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub async fn register_node(&self, node_ref: NodeRef) -> Result<(), ConnectError> {
        let id = node_ref.id();
        let mut connections = self.connections.lock().await;

        if connections.contains_key(&id) {
            Err(ConnectError::AlreadyConnected)
        }
        else if id == self.id {
            Err(ConnectError::ConnectedToSelf)
        } else {
            connections.insert(id, node_ref);
            Ok(())
        }
    }
}

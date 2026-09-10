use crate::_prelude::*;
use std::collections::VecDeque;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SupervisionTree {
    pub description: ChildDescription,
    pub status: Option<ActorStatus>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<Health>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_state: Option<ChannelSnapshot>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SupervisionTree>,
}

impl SupervisionTree {
    pub fn new(description: ChildDescription) -> Self {
        Self {
            status: Registry::local()
                .get(&description.pid)
                .map(|address| address.status()),
            description,
            health: None,
            channel_state: None,
            children: Vec::new(),
        }
    }

    pub async fn populated(mut self, timeout: Duration) -> Self {
        self.populate(timeout).await;
        self
    }

    pub async fn populate(&mut self, timeout: Duration) -> () {
        let mut queue = VecDeque::new();
        queue.push_back(self);

        while let Some(node) = queue.pop_front() {
            node.populate_layer(timeout).await;

            for child in &mut node.children {
                queue.push_back(child);
            }
        }
    }

    async fn populate_layer(&mut self, timeout: Duration) {
        let Some(address) = Registry::local().get(&self.description.pid) else {
            return;
        };

        let children = match tokio::time::timeout(timeout, address.request_dyn(GetChildren)).await {
            Ok(Ok(children)) => children,
            Ok(Err(err)) => {
                tracing::warn!(
                    "Failed to get children for PID {}: {}",
                    self.description.pid,
                    err
                );
                vec![]
            }
            Err(_) => {
                tracing::warn!("Timeout getting children for PID {}", self.description.pid);
                vec![]
            }
        };

        for child in children {
            self.children.push(SupervisionTree::new(child));
        }
    }

    // pub async fn populate_health(&mut self, timeout: Duration) {
    //     let mut queue = VecDeque::new();
    //     queue.push_back(self);

    //     while let Some(node) = queue.pop_front() {
    //         let address = match Registry::local().get(&node.description.pid) {
    //             Some(address) => address,
    //             None => {
    //                 continue;
    //             }
    //         };

    //         let Ok(Ok(debug_state)) =
    //             tokio::time::timeout(timeout, address.request_dyn(GetDebugInfo)).await
    //         else {
    //             continue;
    //         };

    //         node.health = Some(debug_state);

    //         for child in &mut node.children {
    //             queue.push_back(child);
    //         }
    //     }
    // }

    // pub async fn populated_debug_state(mut self, timeout: Duration) -> Self {
    //     self.populate_health(timeout).await;
    //     self
    // }

    pub fn populate_channel_snapshots(&mut self) {
        let mut queue = VecDeque::new();
        queue.push_back(self);

        while let Some(node) = queue.pop_front() {
            let address = match Registry::local().get(&node.description.pid) {
                Some(address) => address,
                None => {
                    continue;
                }
            };

            let channel_state = address.snapshot();
            node.channel_state = Some(channel_state);

            for child in &mut node.children {
                queue.push_back(child);
            }
        }
    }

    pub fn populated_channel_snapshots(mut self) -> Self {
        self.populate_channel_snapshots();
        self
    }
}

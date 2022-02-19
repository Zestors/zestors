use uuid::Uuid;

pub mod node;
pub mod local_node;
pub mod server;
pub mod ws_message;
pub mod challenge;
pub mod raft;
pub mod pid;
pub mod ws_stream;
pub mod cluster;
pub mod registry;


pub type NodeId = u64;
pub type Token = Uuid;
pub type BuildId = Uuid;
pub type ProcessId = Uuid;
pub type MessageId = u32;
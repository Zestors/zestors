use uuid::Uuid;

pub mod local_node;
pub mod server;
pub mod msg;
pub mod challenge;
pub mod pid;
pub mod ws_stream;
pub mod cluster;
pub mod registry;
pub mod node;
pub mod remote_action;


pub type NodeId = u32;
pub type Token = Uuid;
pub type BuildId = Uuid;
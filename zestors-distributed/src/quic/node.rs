use crate::*;
use futures::StreamExt;
use quinn::{Connection, Endpoint, Incoming, IncomingBiStreams, IncomingUniStreams};
use std::{error::Error, io, net::SocketAddr};
use tokio::task::JoinHandle;



struct NodeActor {
    bi_streams: IncomingBiStreams,
    uni_streams: IncomingUniStreams,
}

#[derive(Clone)]
pub struct NodeRef {
    conn: Connection,
}

pub async fn main() -> Result<(), Box<dyn Error>> {
    let addr = "127.0.0.1:5000".parse().unwrap();

    tokio::spawn(async move {
        let cfg = config::insecure_server();
        let (endpoint, mut incoming) = Endpoint::server(cfg, addr).unwrap();

        // accept a single connection
        let incoming_conn = incoming.next().await.unwrap();
        let new_conn = incoming_conn.await.unwrap();
        println!(
            "[server] connection accepted: addr={}",
            new_conn.connection.remote_address()
        );
    });

    let client_cfg = config::insecure_client();
    let mut endpoint = Endpoint::client("127.0.0.1:0".parse().unwrap())?;
    endpoint.set_default_client_config(client_cfg);

    // connect to server
    let quinn::NewConnection { connection, .. } =
        endpoint.connect(addr, "localhost").unwrap().await.unwrap();
    println!("[client] connected: addr={}", connection.remote_address());
    // Dropping handles allows the corresponding objects to automatically shut down
    drop(connection);
    // Make sure the server has a chance to clean up
    endpoint.wait_idle().await;

    Ok(())
}

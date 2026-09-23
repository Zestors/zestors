//! Runs one node of a local cluster and prints membership changes.
//!
//! Start the first node, then more that point at it (in separate terminals):
//!
//! ```text
//! cargo run --example cluster -- node-a 127.0.0.1:7001
//! cargo run --example cluster -- node-b 127.0.0.1:7002 node-a=127.0.0.1:7001
//! cargo run --example cluster -- node-c 127.0.0.1:7003 node-a=127.0.0.1:7001
//! ```
//!
//! Press Ctrl+C on one node to see the others notice it leave, or kill it
//! with `kill -9` to see them detect the crash after a few seconds.
use std::net::SocketAddr;
use zestors::{prelude::*, supervisor::Supervisor};

#[tokio::main]
async fn main() -> Result<(), ClusterNodeError> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let mut args = std::env::args().skip(1);
    let name = args
        .next()
        .expect("usage: cluster <name> <bind> [seed-name=seed-addr]...");
    let bind: SocketAddr = args.next().expect("missing bind address").parse().unwrap();

    // ANCHOR: config
    // Development only: use `Tls::from_pem` to authenticate cluster members.
    let mut config = ClusterConfig::new(name, Quic::new(bind, Tls::insecure_dev().unwrap()));
    for seed in args {
        let (seed_name, seed_addr) = seed.split_once('=').expect("seed must be name=addr");
        config = config.seed(Seed::new(
            seed_name,
            seed_addr.parse::<SocketAddr>().unwrap(),
        ));
    }

    let node = ClusterNode::new(Supervisor::blueprint().rand_name(), config);

    let mut events = node.cluster().subscribe();
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            tracing::info!("{event:?}");
        }
    });

    node.run().await
    // ANCHOR_END: config
}

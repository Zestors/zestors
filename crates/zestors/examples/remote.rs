//! Two nodes in one process: one hosts an actor, the other calls it.
//!
//! Every message that goes between nodes is `Serialize`, `Deserialize` and has
//! a `StableId`; the actor's node also has to register the messages it accepts.
//! Run it with `cargo run --example remote`.
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, time::Duration};
use zestors::{
    distr::{ClusterConfig, ClusterNode, GlobalName, Seed, Tls},
    interface::{Envelope, Interface, Message},
    prelude::*,
    runtime::{Inbox, Name, spawn},
    supervisor::Supervisor,
};

#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = String, id = "3e4c1b7a-0d51-4f0e-9b6f-2a7c5d8e9f10")]
struct Greet(String);

#[derive(Interface, Debug)]
enum GreeterInterface {
    Greet(Envelope<Greet>),
}

fn node(name: &str, addr: SocketAddr, seed: Option<(&str, SocketAddr)>) -> ClusterNode {
    // Development only: use `Tls::from_pem` to authenticate cluster members.
    let mut config = ClusterConfig::new(name, addr, Tls::insecure_dev().unwrap());
    if let Some((seed, seed_addr)) = seed {
        config = config.seed(Seed::new(seed, seed_addr));
    }
    ClusterNode::new(Supervisor::blueprint().rand_name(), config).with_exit_delay(Duration::ZERO)
}

#[tokio::main]
async fn main() {
    let (a_addr, b_addr) = (
        "127.0.0.1:7101".parse().unwrap(),
        "127.0.0.1:7102".parse().unwrap(),
    );
    let host = node("host", a_addr, None);
    let caller = node("caller", b_addr, Some(("host", a_addr)));

    // The host accepts `Greet` from other nodes, and runs an actor that handles it.
    host.remote().register::<Greet>();
    let _greeter = spawn(
        Name::new_static("greeter"),
        |mut inbox: Inbox<GreeterInterface>| async move {
            while let Some(GreeterInterface::Greet(envelope)) = inbox.recv().await {
                let greeting = format!("Hello, {}!", envelope.msg.0);
                let _ = envelope.reply(greeting);
            }
            Ok(())
        },
    )
    .unwrap();

    let (caller_cluster, host_shutdown, caller_shutdown) = (
        caller.cluster(),
        host.shutdown_handle(),
        caller.shutdown_handle(),
    );
    let greeter = caller
        .remote()
        .address::<GreeterInterface>(GlobalName::new("greeter", "host"));
    let (host_task, caller_task) = (tokio::spawn(host.run()), tokio::spawn(caller.run()));

    // Wait until the caller knows the host, then call across.
    caller_cluster.wait_for_members(1).await;
    let greeting = greeter.call(Greet("world".into())).await.unwrap();
    println!("{greeting}");

    host_shutdown.shutdown();
    caller_shutdown.shutdown();
    let _ = tokio::join!(host_task, caller_task);
}

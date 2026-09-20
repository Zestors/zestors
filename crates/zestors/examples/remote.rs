//! Two nodes in one process: one hosts an actor, the other calls it.
//!
//! Every message that goes between nodes is `Serialize`, `Deserialize` and has
//! a `StableId`; the actor's node also has to register the messages it accepts.
//! Run it with `cargo run --example remote`.
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use zestors::{
    distr::{ClusterConfig, ClusterNode, GlobalName, RemoteAccepts, RemoteRequest, Seed},
    distr_quic::{Quic, Tls},
    interface::{Envelope, Interface, Message},
    prelude::*,
    runtime::{Inbox, Name, spawn},
    supervisor::Supervisor,
};

#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = String, id = "3e4c1b7a-0d51-4f0e-9b6f-2a7c5d8e9f10")]
struct Greet(String);

/// A reply channel can be part of a message too: the answer is sent back to
/// whoever holds the other end, on the other node.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(id = "8d2f6a10-5b7e-4c39-a1d4-6e0b9c3f7a21")]
struct CountLetters {
    text: String,
    reply: RemoteRequest<usize>,
}

#[derive(Interface, Debug)]
enum GreeterInterface {
    Greet(Envelope<Greet>),
    CountLetters(Envelope<CountLetters>),
}

fn node(name: &str, addr: SocketAddr, seed: Option<(&str, SocketAddr)>) -> ClusterNode {
    // Development only: use `Tls::from_pem` to authenticate cluster members.
    // Both nodes accept these from others: the host to serve them, the caller
    // so it can name them when it asks the host for an address.
    let mut config = ClusterConfig::new(name, Quic::new(addr, Tls::insecure_dev().unwrap()))
        .register::<Greet>()
        .register::<CountLetters>();
    if let Some((seed, seed_addr)) = seed {
        config = config.seed(Seed::new(seed, seed_addr));
    }
    ClusterNode::new(Supervisor::blueprint().rand_name(), config)
}

#[tokio::main]
async fn main() {
    let (a_addr, b_addr) = (
        "127.0.0.1:7101".parse().unwrap(),
        "127.0.0.1:7102".parse().unwrap(),
    );
    let host = node("host", a_addr, None);
    let caller = node("caller", b_addr, Some(("host", a_addr)));

    // The host runs the actor that handles them.
    let _greeter = spawn(
        Name::new_static("greeter"),
        |mut inbox: Inbox<GreeterInterface>| async move {
            while let Some(msg) = inbox.recv().await {
                match msg {
                    GreeterInterface::Greet(envelope) => {
                        let greeting = format!("Hello, {}!", envelope.msg.0);
                        let _ = envelope.reply(greeting);
                    }
                    GreeterInterface::CountLetters(envelope) => {
                        let CountLetters { text, reply } = envelope.msg;
                        let _ = reply.reply(text.chars().count());
                    }
                }
            }
            Ok(())
        },
    )
    .unwrap();

    let host_remote = host.cluster();
    let (caller_cluster, host_shutdown, caller_shutdown) = (
        caller.cluster(),
        host.root_supervisor().address().clone(),
        caller.root_supervisor().address().clone(),
    );
    let (host_task, caller_task) = (tokio::spawn(host.run()), tokio::spawn(caller.run()));

    // Addressing an actor on another node asks that node, so wait until the caller knows the host.
    caller_cluster.wait_for_members(1).await;
    let greeter = caller_cluster
        .address::<GreeterInterface>(GlobalName::new("greeter", "host"))
        .await
        .unwrap();
    // The same for an actor on this node: the host's own view of its greeter is
    // delivered locally, without leaving the process.
    let local_greeter = host_remote
        .address::<GreeterInterface>(GlobalName::new("greeter", "host"))
        .await
        .unwrap();

    let greeting = greeter.call(Greet("world".into())).await.unwrap();
    println!("{greeting}");
    println!(
        "{}",
        local_greeter.call(Greet("host".into())).await.unwrap()
    );

    // A message with a reply channel in it: keep the `Reply`, send the request.
    let (reply, count) = RemoteRequest::new();
    greeter
        .cast(CountLetters {
            text: "world".into(),
            reply,
        })
        .await
        .unwrap();
    println!("{} letters", count.await.unwrap());

    stop(&host_shutdown).await;
    stop(&caller_shutdown).await;
    let _ = tokio::join!(host_task, caller_task);
}

/// Shuts a node down through its root supervisor, once that takes signals: it
/// is initializing or running. Sooner, they are dropped.
async fn stop(root: &zestors::runtime::Address<zestors::supervisor::SupervisorInterface>) {
    root.watch_accepts_messages().await;
    root.signal_shutdown();
}

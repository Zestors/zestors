//! Two nodes in one process: one hosts an actor, the other calls it.
//!
//! Every message that goes between nodes is `Serialize`, `Deserialize` and has
//! a `StableId`; the actor's node also has to register the messages it accepts.
//! Run it with `cargo run --example remote`.
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, time::Duration};
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
    let mut config = ClusterConfig::new(name, Quic::new(addr, Tls::insecure_dev().unwrap()));
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

    // The host accepts these messages from other nodes, and runs an actor that handles it.
    host.cluster().register::<Greet>().register::<CountLetters>();
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
        host.shutdown_handle(),
        caller.shutdown_handle(),
    );
    let greeter = caller
        .cluster()
        .address::<GreeterInterface>(GlobalName::new("greeter", "host"))
        .unwrap();
    // An address that works the same for an actor on this node: the host's own
    // view of its greeter is delivered locally, without leaving the process.
    let local_greeter = host_remote
        .cluster_address::<GreeterInterface>(GlobalName::new("greeter", "host"))
        .unwrap();
    let (host_task, caller_task) = (tokio::spawn(host.run()), tokio::spawn(caller.run()));

    // Wait until the caller knows the host, then call across.
    caller_cluster.wait_for_members(1).await;
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

    host_shutdown.shutdown();
    caller_shutdown.shutdown();
    let _ = tokio::join!(host_task, caller_task);
}

//! Two cluster nodes in one process, talking over QUIC on localhost: `host`
//! runs an actor, `caller` calls it.
//!
//! Run it with `cargo run -p zestors --example remote`. It prints two
//! greetings and "5 letters", then shuts both nodes down.
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use zestors::{
    interface::{Envelope, Interface, Message},
    prelude::*,
    runtime::{Inbox, Name, spawn},
    supervisor::{Supervisor, SupervisorInterface},
};

// ANCHOR: messages
/// A message that can cross the network: it has a stable id, and serde.
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
// ANCHOR_END: messages

fn config(name: &str, addr: SocketAddr) -> ClusterConfig {
    // Development only: use `Tls::from_pem` to authenticate cluster members.
    ClusterConfig::new(name, Quic::new(addr, Tls::insecure_dev().unwrap()))
}

#[tokio::main]
async fn main() {
    let (host_addr, caller_addr): (SocketAddr, SocketAddr) = (
        "127.0.0.1:7101".parse().unwrap(),
        "127.0.0.1:7102".parse().unwrap(),
    );

    // ANCHOR: nodes
    // The host serves the greeter, so it registers the messages the greeter
    // accepts from other nodes. The caller only sends them, which needs no
    // registration.
    let host = ClusterNode::new(
        Supervisor::blueprint().rand_name(),
        config("host", host_addr)
            .register::<Greet>()
            .register::<CountLetters>(),
    );
    let caller = ClusterNode::new(
        Supervisor::blueprint().rand_name(),
        config("caller", caller_addr).seed(Seed::new("host", host_addr)),
    );
    // ANCHOR_END: nodes

    // The greeter runs on the host. Any actor registered under a `Name` can be
    // reached from other nodes.
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

    // `run` consumes the node, so take what is needed from it first.
    let (host_cluster, caller_cluster) = (host.cluster(), caller.cluster());
    let (host_root, caller_root) = (
        host.root_supervisor().address().clone(),
        caller.root_supervisor().address().clone(),
    );
    let (host_task, caller_task) = (tokio::spawn(host.run()), tokio::spawn(caller.run()));

    // ANCHOR: call
    // Addressing an actor on another node asks that node, so first wait until
    // the caller has joined the host.
    caller_cluster.wait_for_members(1).await;
    let greeter = caller_cluster
        .address::<GreeterInterface>(GlobalName::new("greeter", "host"))
        .await
        .unwrap();
    println!("{}", greeter.call(Greet("world".into())).await.unwrap());

    // The same works for an actor on this node: the host's own address for its
    // greeter delivers locally, without encoding the message.
    let local_greeter = host_cluster
        .address::<GreeterInterface>(GlobalName::new("greeter", "host"))
        .await
        .unwrap();
    println!(
        "{}",
        local_greeter.call(Greet("host".into())).await.unwrap()
    );

    // A message with a reply channel in it: send the `RemoteRequest` inside the
    // message, and wait on the `Reply`.
    let (request, reply) = RemoteRequest::new();
    greeter
        .cast(CountLetters {
            text: "world".into(),
            reply: request,
        })
        .await
        .unwrap();
    println!("{} letters", reply.await.unwrap());
    // ANCHOR_END: call

    stop(&host_root).await;
    stop(&caller_root).await;
    let _ = tokio::join!(host_task, caller_task);
}

/// Shuts a node down through its root supervisor. A signal sent before the
/// supervisor has started is dropped, so wait for it to accept one first.
async fn stop(root: &Address<SupervisorInterface>) {
    root.monitor_accepts_messages().await;
    root.signal_shutdown();
}

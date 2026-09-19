//! Messages to actors on other nodes, over real QUIC.

use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::Duration,
};
use zestors::{
    interface::{Envelope, Interface, Message},
    prelude::*,
    runtime::{Inbox, Name, spawn},
    supervisor::Supervisor,
};
use zestors_distr::{
    ClusterConfig, ClusterNode, GlobalName, RemoteAddress, Seed, StableId, Tls,
    backend::{Quic, QuicTimings},
};

#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = String, id = "2f8a6a52-63d3-4c0e-9c5b-8a4d5d7e1b01")]
struct Greet(String);

#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(id = "2f8a6a52-63d3-4c0e-9c5b-8a4d5d7e1b02")]
struct Note(u32);

#[derive(Interface, Debug)]
enum GreeterInterface {
    Greet(Envelope<Greet>),
    Note(Envelope<Note>),
}

fn free_addr() -> SocketAddr {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
}

fn node(name: &str, addr: SocketAddr, seed: Option<(&str, SocketAddr)>) -> ClusterNode {
    let mut foca = foca::Config::new_lan(NonZeroU32::new(3).unwrap());
    foca.probe_period = Duration::from_millis(100);
    foca.probe_rtt = Duration::from_millis(50);
    foca.suspect_to_down_after = Duration::from_millis(200);
    let quic = Quic::new(addr, Tls::insecure_dev().unwrap()).timings(QuicTimings {
        keep_alive: Duration::from_millis(200),
        idle_timeout: Duration::from_secs(1),
    });
    let mut config = ClusterConfig::with_backend(name, quic).foca_config(foca);
    if let Some((seed, seed_addr)) = seed {
        config = config.seed(Seed::new(seed, seed_addr));
    }
    ClusterNode::new(Supervisor::blueprint().rand_name(), config).with_exit_delay(Duration::ZERO)
}

#[tokio::test(flavor = "multi_thread")]
async fn actors_on_another_node_can_be_called_and_cast_to() {
    let (a_addr, b_addr) = (free_addr(), free_addr());
    let a = node("node-a", a_addr, None);
    let b = node("node-b", b_addr, Some(("node-a", a_addr)));
    let (a_remote, b_remote) = (a.remote(), b.remote());
    let (a_cluster, a_shutdown, b_shutdown) =
        (a.cluster(), a.shutdown_handle(), b.shutdown_handle());
    let (a_task, b_task) = (tokio::spawn(a.run()), tokio::spawn(b.run()));

    b_remote.register::<Greet>().register::<Note>();
    tokio::time::timeout(Duration::from_secs(30), a_cluster.wait_for_members(1))
        .await
        .expect("The nodes find each other");

    // An actor on node-b.
    let notes = Arc::new(Mutex::new(Vec::new()));
    let _greeter = spawn(
        Name::new_static("quic-greeter"),
        |mut inbox: Inbox<GreeterInterface>| {
            let notes = notes.clone();
            async move {
                while let Some(msg) = inbox.recv().await {
                    match msg {
                        GreeterInterface::Greet(envelope) => {
                            let hello = format!("Hello, {}!", envelope.msg.0);
                            let _ = envelope.reply(hello);
                        }
                        GreeterInterface::Note(envelope) => {
                            notes.lock().unwrap().push(envelope.msg.0)
                        }
                    }
                }
                Ok(())
            }
        },
    )
    .unwrap();

    let greeter: RemoteAddress<GreeterInterface> =
        a_remote.address(GlobalName::new("quic-greeter", "node-b"));
    assert_eq!(
        greeter.call(Greet("QUIC".into())).await.unwrap(),
        "Hello, QUIC!"
    );

    for i in 0..500 {
        greeter.cast(Note(i)).await.unwrap();
    }
    // A call is behind every cast on the same pipe.
    greeter.call(Greet("done".into())).await.unwrap();
    assert_eq!(*notes.lock().unwrap(), (0..500).collect::<Vec<_>>());

    a_shutdown.shutdown();
    b_shutdown.shutdown();
    a_task.await.unwrap().unwrap();
    b_task.await.unwrap().unwrap();
}

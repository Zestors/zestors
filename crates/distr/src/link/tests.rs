//! The links over the simulated network: what the layers above rely on, end to end.

use super::*;
use crate::{Member, sim::SimNetwork};
use std::time::Duration;
use tokio::{sync::broadcast, time::timeout};

/// A protocol for the tests to talk on.
const TEST: Protocol = Protocol(20);

/// A node bound to its own address, with its inbox for [`TEST`] and its peer reports.
struct TestNode {
    links: Links,
    inbox: mpsc::Receiver<Incoming>,
    peers: broadcast::Receiver<PeerEvent>,
    member: Member,
}

/// The address a node of the tests listens on.
fn addr_of(name: &str) -> NodeAddr {
    NodeAddr::new(format!("sim:{name}"))
}

async fn bind(net: &SimNetwork, name: &str) -> TestNode {
    bind_at(net, name, addr_of(name), LinkTimings::default()).await
}

async fn bind_at(
    net: &SimNetwork,
    name: &str,
    addr: NodeAddr,
    timings: LinkTimings,
) -> TestNode {
    let backend = net.backend(addr);
    let local = LocalNode {
        id: NodeId::new(name),
        generation: 1,
    };
    let (links, addr) = Starter::new(backend).start(local, timings).await.unwrap();
    let peers = links.peer_events();
    let inbox = links.subscribe(TEST);
    TestNode {
        links,
        inbox,
        peers,
        member: Member {
            node: NodeId::new(name),
            addr,
            generation: 1,
        },
    }
}

async fn next(inbox: &mut mpsc::Receiver<Incoming>) -> Incoming {
    timeout(Duration::from_secs(5), inbox.recv())
        .await
        .expect("message arrives")
        .expect("links are open")
}

#[tokio::test(start_paused = true)]
async fn datagrams_arrive_small_or_big() {
    let net = SimNetwork::new(1);
    let a = bind(&net, "node-a").await;
    let mut b = bind(&net, "node-b").await;
    let sender = a
        .links
        .sender(&b.member.node, &b.member.addr, TEST, Delivery::Datagram, 0);

    let small = Bytes::from(vec![7u8; 100]);
    sender.send(small.clone()).await.unwrap();
    let received = next(&mut b.inbox).await;
    assert_eq!(received.payload, small);
    assert_eq!(received.from.as_str(), "node-a");
    assert_eq!(received.generation, 1);

    // Too big for any datagram: goes over a stream instead.
    let large = Bytes::from(vec![9u8; 20_000]);
    sender.send(large.clone()).await.unwrap();
    assert_eq!(next(&mut b.inbox).await.payload, large);
}

#[tokio::test(start_paused = true)]
async fn ordered_messages_arrive_in_order() {
    let net = SimNetwork::new(1);
    let a = bind(&net, "node-a").await;
    let mut b = bind(&net, "node-b").await;

    let sender = a
        .links
        .sender(&b.member.node, &b.member.addr, TEST, Delivery::Ordered, 0);
    // Enough that they queue up, and of mixed sizes. Sent alongside reading:
    // sending waits for room, which the reader makes.
    tokio::spawn(async move {
        for i in 0u32..2_000 {
            let mut payload = i.to_be_bytes().to_vec();
            payload.resize(4 + (i as usize % 7) * 900, 0);
            sender.send(Bytes::from(payload)).await.unwrap();
        }
    });
    for i in 0u32..2_000 {
        let received = next(&mut b.inbox).await;
        assert_eq!(
            received.payload[..4],
            i.to_be_bytes(),
            "message {i} in order"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn large_messages_get_through_and_oversized_ones_are_dropped() {
    let net = SimNetwork::new(1);
    let a = bind(&net, "node-a").await;
    let mut b = bind(&net, "node-b").await;
    let sender = a
        .links
        .sender(&b.member.node, &b.member.addr, TEST, Delivery::Ordered, 0);

    let large = Bytes::from(vec![3u8; MAX_MESSAGE_SIZE]);
    sender.send(large.clone()).await.unwrap();
    assert_eq!(next(&mut b.inbox).await.payload, large);

    // Over the limit: dropped, and what follows is unaffected.
    sender
        .send(Bytes::from(vec![0u8; MAX_MESSAGE_SIZE + 1]))
        .await
        .unwrap();
    sender.send(Bytes::from_static(b"after")).await.unwrap();
    assert_eq!(next(&mut b.inbox).await.payload, "after");
}

#[tokio::test(start_paused = true)]
async fn protocols_do_not_mix() {
    let net = SimNetwork::new(1);
    let a = bind(&net, "node-a").await;
    let mut b = bind(&net, "node-b").await;
    let mut other = b.links.subscribe(Protocol(21));

    for (protocol, payload) in [(TEST, "one"), (Protocol(21), "two"), (TEST, "three")] {
        let sender = a.links.sender(
            &b.member.node,
            &b.member.addr,
            protocol,
            Delivery::Ordered,
            0,
        );
        sender.send(Bytes::from(payload)).await.unwrap();
    }
    assert_eq!(next(&mut b.inbox).await.payload, "one");
    assert_eq!(next(&mut b.inbox).await.payload, "three");
    assert_eq!(next(&mut other).await.payload, "two");
}

/// Two nodes that start talking to each other at the same moment end up with
/// one connection, and lose nothing that was sent afterwards.
#[tokio::test(start_paused = true)]
async fn nodes_dialing_each_other_at_once_settle_on_one_connection() {
    let net = SimNetwork::new(1);
    let mut a = bind(&net, "node-a").await;
    let mut b = bind(&net, "node-b").await;

    let (to_b, to_a) = (
        a.links
            .sender(&b.member.node, &b.member.addr, TEST, Delivery::Ordered, 0),
        b.links
            .sender(&a.member.node, &a.member.addr, TEST, Delivery::Ordered, 0),
    );
    to_b.send(Bytes::from_static(b"first")).await.unwrap();
    to_a.send(Bytes::from_static(b"first")).await.unwrap();
    // The first ones may be lost in the race; what follows must arrive.
    tokio::time::sleep(Duration::from_millis(300)).await;
    for _ in 0..50 {
        to_b.send(Bytes::from_static(b"later")).await.unwrap();
        to_a.send(Bytes::from_static(b"later")).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    for inbox in [&mut a.inbox, &mut b.inbox] {
        timeout(Duration::from_secs(5), async {
            loop {
                if inbox.recv().await.expect("open").payload == "later" {
                    break;
                }
            }
        })
        .await
        .expect("messages sent after the race arrive");
    }
}

#[tokio::test(start_paused = true)]
async fn losing_a_connection_is_reported() {
    let net = SimNetwork::new(1);
    let mut a = bind(&net, "node-a").await;
    let mut b = bind(&net, "node-b").await;
    let sender = a
        .links
        .sender(&b.member.node, &b.member.addr, TEST, Delivery::Ordered, 0);
    sender.send(Bytes::from_static(b"hi")).await.unwrap();
    next(&mut b.inbox).await;

    // b goes away.
    drop(b);
    let event = timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(PeerEvent::Disconnected { node, generation }) = a.peers.recv().await {
                break (node, generation);
            }
        }
    })
    .await
    .expect("disconnect is reported");
    assert_eq!(event, (NodeId::new("node-b"), 1));
}

#[tokio::test(start_paused = true)]
async fn unreachable_peer_is_reported_and_reported_reachable_when_it_answers() {
    let net = SimNetwork::new(1);
    let timings = LinkTimings {
        connect_timeout: Duration::from_millis(100),
        reconnect_backoff_min: Duration::from_millis(20),
        reconnect_backoff_max: Duration::from_millis(100),
        ..LinkTimings::default()
    };
    let mut a = bind_at(&net, "node-a", addr_of("node-a"), timings.clone()).await;

    // An address nobody listens on yet.
    let addr = addr_of("node-b");
    let b_member = Member {
        node: NodeId::new("node-b"),
        addr: addr.clone(),
        generation: 1,
    };

    // Keep trying to reach it; messages sent while backing off are dropped.
    async fn until(a: &mut TestNode, to: &Member, wanted: impl Fn(&PeerEvent) -> bool) {
        timeout(Duration::from_secs(10), async {
            let mut tick = tokio::time::interval(Duration::from_millis(20));
            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        let _ = a.links.sender(&to.node, &to.addr, TEST, Delivery::Datagram, 0).try_send(Bytes::from_static(b"hi"));
                    }
                    Ok(event) = a.peers.recv() => if wanted(&event) { return },
                }
            }
        })
        .await
        .expect("expected event");
    }

    until(
        &mut a,
        &b_member,
        |e| matches!(e, PeerEvent::Unreachable(n) if n.as_str() == "node-b"),
    )
    .await;

    // The peer comes up at that address.
    let _b = bind_at(&net, "node-b", addr, timings).await;
    until(
        &mut a,
        &b_member,
        |e| matches!(e, PeerEvent::Reachable(n) if n.as_str() == "node-b"),
    )
    .await;
}

//! The links over real QUIC: what the layers above rely on, end to end.

use super::*;
use crate::backend::{Quic, Tls};
use std::time::Duration;
use tokio::time::timeout;

/// A protocol for the tests to talk on.
const TEST: Protocol = Protocol(20);

/// A node bound to a free port, with its inbox for [`TEST`] and its peer reports.
struct TestNode {
    links: Links,
    inbox: mpsc::Receiver<Incoming>,
    peers: mpsc::Receiver<PeerEvent>,
    member: Member,
}

async fn bind(name: &str) -> TestNode {
    bind_at(
        name,
        "127.0.0.1:0".parse().unwrap(),
        ClusterTimings::default(),
    )
    .await
}

async fn bind_at(name: &str, addr: std::net::SocketAddr, timings: ClusterTimings) -> TestNode {
    let backend = Quic::new(addr, Tls::insecure_dev().unwrap());
    let local = LocalNode {
        id: NodeId::new(name),
        generation: 1,
    };
    let (links, addr, peers) = Starter::new(backend).start(local, timings).await.unwrap();
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

#[tokio::test]
async fn datagrams_arrive_small_or_big() {
    let a = bind("node-a").await;
    let mut b = bind("node-b").await;
    let sender = a.links.sender(&b.member, TEST, Delivery::Datagram);

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

#[tokio::test]
async fn ordered_messages_arrive_in_order() {
    let a = bind("node-a").await;
    let mut b = bind("node-b").await;

    let sender = a.links.sender(&b.member, TEST, Delivery::Ordered);
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

#[tokio::test]
async fn large_messages_get_through_and_oversized_ones_are_dropped() {
    let a = bind("node-a").await;
    let mut b = bind("node-b").await;
    let sender = a.links.sender(&b.member, TEST, Delivery::Ordered);

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

#[tokio::test]
async fn protocols_do_not_mix() {
    let a = bind("node-a").await;
    let mut b = bind("node-b").await;
    let mut other = b.links.subscribe(Protocol(21));

    for (protocol, payload) in [(TEST, "one"), (Protocol(21), "two"), (TEST, "three")] {
        let sender = a.links.sender(&b.member, protocol, Delivery::Ordered);
        sender.send(Bytes::from(payload)).await.unwrap();
    }
    assert_eq!(next(&mut b.inbox).await.payload, "one");
    assert_eq!(next(&mut b.inbox).await.payload, "three");
    assert_eq!(next(&mut other).await.payload, "two");
}

/// Two nodes that start talking to each other at the same moment end up with
/// one connection, and lose nothing that was sent afterwards.
#[tokio::test]
async fn nodes_dialing_each_other_at_once_settle_on_one_connection() {
    let mut a = bind("node-a").await;
    let mut b = bind("node-b").await;

    let (to_b, to_a) = (
        a.links.sender(&b.member, TEST, Delivery::Ordered),
        b.links.sender(&a.member, TEST, Delivery::Ordered),
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

#[tokio::test]
async fn losing_a_connection_is_reported() {
    let mut a = bind("node-a").await;
    let mut b = bind("node-b").await;
    let sender = a.links.sender(&b.member, TEST, Delivery::Ordered);
    sender.send(Bytes::from_static(b"hi")).await.unwrap();
    next(&mut b.inbox).await;

    // b goes away.
    drop(b);
    let event = timeout(Duration::from_secs(10), async {
        loop {
            if let PeerEvent::Disconnected { node, generation } =
                a.peers.recv().await.expect("links are open")
            {
                break (node, generation);
            }
        }
    })
    .await
    .expect("disconnect is reported");
    assert_eq!(event, (NodeId::new("node-b"), 1));
}

#[tokio::test]
async fn unreachable_peer_is_reported_and_reported_reachable_when_it_answers() {
    let timings = ClusterTimings {
        connect_timeout: Duration::from_millis(100),
        reconnect_backoff_min: Duration::from_millis(20),
        reconnect_backoff_max: Duration::from_millis(100),
        ..ClusterTimings::default()
    };
    let mut a = bind_at("node-a", "127.0.0.1:0".parse().unwrap(), timings.clone()).await;

    // An address nobody listens on yet.
    let addr = std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let b_member = Member {
        node: NodeId::new("node-b"),
        addr: addr.into(),
        generation: 1,
    };

    // Keep trying to reach it; messages sent while backing off are dropped.
    async fn until(a: &mut TestNode, to: &Member, wanted: impl Fn(&PeerEvent) -> bool) {
        timeout(Duration::from_secs(10), async {
            let mut tick = tokio::time::interval(Duration::from_millis(20));
            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        let _ = a.links.sender(to, TEST, Delivery::Datagram).try_send(Bytes::from_static(b"hi"));
                    }
                    Some(event) = a.peers.recv() => if wanted(&event) { return },
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
    let _b = bind_at("node-b", addr, timings).await;
    until(
        &mut a,
        &b_member,
        |e| matches!(e, PeerEvent::Reachable(n) if n.as_str() == "node-b"),
    )
    .await;
}

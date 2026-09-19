use super::hello::{Hello, HelloError, KIND_HELLO, PROTOCOL_VERSION};
use super::*;
use tokio::time::timeout;

#[test]
fn hello_round_trips() {
    let hello = Hello {
        node: NodeId::new("node-a"),
        generation: 42,
    };
    let decoded = Hello::decode(&hello.encode()).unwrap();
    assert_eq!(decoded.node, hello.node);
    assert_eq!(decoded.generation, 42);
}

#[test]
fn hello_with_other_version_is_refused_before_parsing_the_rest() {
    // A future version may lay out the remainder differently: even an
    // otherwise unparseable body must be reported as a version mismatch.
    let mut bytes = vec![KIND_HELLO];
    bytes.extend_from_slice(&(PROTOCOL_VERSION + 1).to_be_bytes());
    assert!(matches!(
        Hello::decode(&bytes),
        Err(HelloError::Version(v)) if v == PROTOCOL_VERSION + 1
    ));
}

#[test]
fn malformed_hellos_are_refused() {
    assert!(matches!(Hello::decode(&[]), Err(HelloError::Invalid)));
    assert!(matches!(
        Hello::decode(&[KIND_HELLO + 1, 0, 1]),
        Err(HelloError::Invalid)
    ));
    let mut truncated = vec![KIND_HELLO];
    truncated.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    truncated.extend_from_slice(&[0; 4]);
    assert!(matches!(
        Hello::decode(&truncated),
        Err(HelloError::Invalid)
    ));
}

fn bind(name: &str) -> (Transport, mpsc::Receiver<Event>, Member) {
    bind_at(
        name,
        "127.0.0.1:0".parse().unwrap(),
        ClusterTimings::default(),
    )
}

fn bind_at(
    name: &str,
    addr: SocketAddr,
    timings: ClusterTimings,
) -> (Transport, mpsc::Receiver<Event>, Member) {
    let (transport, events) = Transport::bind(
        addr,
        &Tls::insecure_dev().unwrap(),
        NodeId::new(name),
        1,
        timings,
    )
    .unwrap();
    let member = Member {
        node: NodeId::new(name),
        addr: transport.local_addr().unwrap(),
        generation: 1,
    };
    (transport, events, member)
}

async fn next_gossip(events: &mut mpsc::Receiver<Event>) -> Bytes {
    loop {
        let event = timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("message arrives")
            .expect("transport is open");
        match event {
            Event::Received(Incoming {
                frame: Frame::Gossip(data),
                ..
            }) => return data,
            Event::Received(other) => panic!("expected gossip, got {other:?}"),
            Event::Unreachable(_) | Event::Reachable(_) => {}
        }
    }
}

#[tokio::test]
async fn gossip_is_delivered_as_datagram_or_stream_fallback() {
    let (a, _a_events, _) = bind("node-a");
    let (_b, mut b_events, b_member) = bind("node-b");

    // Small: fits in a datagram.
    let small = Bytes::from(vec![7u8; 100]);
    a.send(&b_member, Frame::Gossip(small.clone()));
    assert_eq!(next_gossip(&mut b_events).await, small);
    let conn = a.inner.live(&b_member.node).expect("connected to b");
    assert_eq!(conn.stats().frame_tx.datagram, 1, "sent as a datagram");

    // Too big for any datagram: falls back to a stream.
    let large = Bytes::from(vec![9u8; 20_000]);
    a.send(&b_member, Frame::Gossip(large.clone()));
    assert_eq!(next_gossip(&mut b_events).await, large);
    assert_eq!(
        conn.stats().frame_tx.datagram,
        1,
        "the large message did not go out as a datagram"
    );
}

#[tokio::test]
async fn unreachable_peer_is_reported_and_reported_reachable_when_it_answers() {
    let timings = ClusterTimings {
        connect_timeout: Duration::from_millis(100),
        reconnect_backoff_min: Duration::from_millis(20),
        reconnect_backoff_max: Duration::from_millis(100),
        ..ClusterTimings::default()
    };
    let (a, mut a_events, _) = bind_at("node-a", "127.0.0.1:0".parse().unwrap(), timings.clone());

    // An address nobody listens on yet.
    let addr = std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let b_member = Member {
        node: NodeId::new("node-b"),
        addr,
        generation: 1,
    };

    // Keep trying to reach it; messages sent while backing off are dropped.
    async fn until(
        a: &Transport,
        to: &Member,
        events: &mut mpsc::Receiver<Event>,
        wanted: impl Fn(&Event) -> bool,
    ) {
        timeout(Duration::from_secs(10), async {
            let mut tick = tokio::time::interval(Duration::from_millis(20));
            loop {
                tokio::select! {
                    _ = tick.tick() => a.send(to, Frame::Gossip(Bytes::from_static(b"hi"))),
                    Some(event) = events.recv() => if wanted(&event) { return },
                }
            }
        })
        .await
        .expect("expected event");
    }

    until(
        &a,
        &b_member,
        &mut a_events,
        |e| matches!(e, Event::Unreachable(n) if n.as_str() == "node-b"),
    )
    .await;

    // The peer comes up at that address.
    let (_b, _b_events, _) = bind_at("node-b", addr, timings);
    until(
        &a,
        &b_member,
        &mut a_events,
        |e| matches!(e, Event::Reachable(n) if n.as_str() == "node-b"),
    )
    .await;
}

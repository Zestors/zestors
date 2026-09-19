//! Messages to actors on other nodes: two cluster nodes over the simulated
//! network, on virtual time.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    num::{NonZeroU8, NonZeroU32},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::task::JoinHandle;
use zestors::{
    interface::{Envelope, Interface, Message},
    prelude::*,
    runtime::{ActorStatus, Dyn, Inbox, Name, spawn},
    supervisor::Supervisor,
};
use zestors_distr::{
    AddressError, Cluster, ClusterAddress, ClusterConfig, ClusterNode, ClusterNodeError, Decode,
    DecodeError, Encode, EncodeError, GlobalName, RemoteAccepts, RemoteActorOps, RemoteAddress,
    RemoteCallError, RemoteCallOptions, RemoteCastError, RemoteError, RemoteOpError,
    RemoteReceipt as _, RemoteReplyError, RemoteRequest, Seed, StableId, sim::SimNetwork,
};

// Messages. All but `Reverse` cross the network with serde.

#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = u32, id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a01")]
struct Double(u32);

#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a02")]
struct Note(u32);

/// The actor never answers this one, and doesn't drop it either.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = u32, id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a03")]
struct Hang;

/// The actor drops this one without answering it.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = u32, id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a04")]
struct Forget;

/// The actor takes this long to answer it.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = u32, id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a05")]
struct Slow(u64);

/// Registered, but the actor doesn't accept it.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = u32, id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a06")]
struct Unwanted;

/// Not registered anywhere.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = u32, id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a07")]
struct Unknown;

#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a08")]
struct Blob(Vec<u8>);

/// A message that has no serde implementation at all, and is put on the wire by
/// hand, together with its reply.
#[derive(Message, StableId, Debug, PartialEq)]
#[msg(reply = Reversed, id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a09")]
struct Reverse(String);

#[derive(Debug, PartialEq)]
struct Reversed(String);

impl Encode for Reverse {
    fn encode(&self) -> Result<Bytes, EncodeError> {
        Ok(Bytes::from(self.0.clone().into_bytes()))
    }
}

impl Decode for Reverse {
    fn decode(bytes: Bytes) -> Result<Self, DecodeError> {
        String::from_utf8(bytes.to_vec())
            .map(Reverse)
            .map_err(DecodeError::new)
    }
}

impl Encode for Reversed {
    fn encode(&self) -> Result<Bytes, EncodeError> {
        Ok(Bytes::from(self.0.clone().into_bytes()))
    }
}

impl Decode for Reversed {
    fn decode(bytes: Bytes) -> Result<Self, DecodeError> {
        String::from_utf8(bytes.to_vec())
            .map(Reversed)
            .map_err(DecodeError::new)
    }
}

/// A reply channel in the message: the actor answers it with `n + 1`.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a0a")]
struct Fetch {
    n: u32,
    reply: RemoteRequest<u32>,
}

/// The actor drops the request in this one.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a0b")]
struct FetchAndForget {
    reply: RemoteRequest<u32>,
}

/// The actor keeps the request without answering it.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a0c")]
struct FetchAndHang {
    reply: RemoteRequest<u32>,
}

/// Has a reply of its own as well as a request: the actor answers the call
/// with `n` and the request with `n + 100`.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(reply = u32, id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a0d")]
struct Both {
    n: u32,
    reply: RemoteRequest<u32>,
}

/// Too large to send, with a request.
#[derive(Message, StableId, Serialize, Deserialize, Debug)]
#[msg(id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a0e")]
struct BigFetch {
    blob: Vec<u8>,
    reply: RemoteRequest<u32>,
}

/// A message that can't be put on the wire at all: it can only be delivered to
/// an actor on the same node.
#[derive(Message, StableId, Debug)]
#[msg(reply = u32, id = "6b0e3f0e-6c2a-4c9b-8f57-0d7a5f8d0a0f")]
struct NoWire(u32);

impl Encode for NoWire {
    fn encode(&self) -> Result<Bytes, EncodeError> {
        Err(EncodeError::new("Never leaves the process"))
    }
}

impl Decode for NoWire {
    fn decode(_: Bytes) -> Result<Self, DecodeError> {
        Err(DecodeError::new("Never leaves the process"))
    }
}

#[derive(Interface, Debug)]
enum WorkerInterface {
    NoWire(Envelope<NoWire>),
    Fetch(Envelope<Fetch>),
    FetchAndForget(Envelope<FetchAndForget>),
    FetchAndHang(Envelope<FetchAndHang>),
    Both(Envelope<Both>),
    BigFetch(Envelope<BigFetch>),
    Double(Envelope<Double>),
    Note(Envelope<Note>),
    Hang(Envelope<Hang>),
    Forget(Envelope<Forget>),
    Slow(Envelope<Slow>),
    Blob(Envelope<Blob>),
    Reverse(Envelope<Reverse>),
}

/// What the worker has been told.
#[derive(Default, Clone)]
struct Log(Arc<Mutex<Vec<u32>>>);

impl Log {
    fn notes(&self) -> Vec<u32> {
        self.0.lock().unwrap().clone()
    }
}

/// Runs a worker actor under `name`.
fn worker(name: &'static str, log: Log) -> zestors::runtime::Child<(), WorkerInterface> {
    spawn(
        Name::new_static(name),
        |mut inbox: Inbox<WorkerInterface>| async move {
            let mut hung = Vec::new();
            let mut hung_requests = Vec::new();
            while let Some(msg) = inbox.recv().await {
                match msg {
                    WorkerInterface::Fetch(envelope) => {
                        let Fetch { n, reply } = envelope.msg;
                        let _ = reply.reply(n + 1);
                    }
                    WorkerInterface::FetchAndForget(envelope) => {
                        envelope.msg.reply.no_reply();
                    }
                    WorkerInterface::FetchAndHang(envelope) => {
                        hung_requests.push(envelope.msg.reply);
                    }
                    WorkerInterface::Both(envelope) => {
                        let Envelope { msg, req } = envelope;
                        let _ = msg.reply.reply(msg.n + 100);
                        let _ = req.reply(msg.n);
                    }
                    WorkerInterface::BigFetch(envelope) => envelope.msg.reply.no_reply(),
                    WorkerInterface::Double(envelope) => {
                        let n = envelope.msg.0;
                        let _ = envelope.reply(n * 2);
                    }
                    WorkerInterface::NoWire(envelope) => {
                        let n = envelope.msg.0;
                        let _ = envelope.reply(n + 1);
                    }
                    WorkerInterface::Note(envelope) => log.0.lock().unwrap().push(envelope.msg.0),
                    WorkerInterface::Hang(envelope) => hung.push(envelope),
                    WorkerInterface::Forget(envelope) => drop(envelope),
                    WorkerInterface::Slow(envelope) => {
                        tokio::time::sleep(Duration::from_millis(envelope.msg.0)).await;
                        let _ = envelope.reply(1);
                    }
                    WorkerInterface::Blob(_) => {}
                    WorkerInterface::Reverse(envelope) => {
                        let reversed = Reversed(envelope.msg.0.chars().rev().collect());
                        let _ = envelope.reply(reversed);
                    }
                }
            }
            Ok(())
        },
    )
    .expect("The name is unused")
}

fn addr(n: u8) -> SocketAddr {
    SocketAddr::from(([10, 0, 0, n], 7000))
}

fn fast_foca() -> foca::Config {
    let mut config = foca::Config::new_lan(NonZeroU32::new(3).unwrap());
    config.probe_period = Duration::from_millis(100);
    config.probe_rtt = Duration::from_millis(50);
    config.suspect_to_down_after = Duration::from_millis(200);
    config
}

struct Node {
    cluster: Cluster,
    shutdown: zestors_supervisor::NodeShutdown,
    task: JoinHandle<Result<(), ClusterNodeError>>,
}

fn node(net: &SimNetwork, name: &str, n: u8, seed: Option<u8>) -> ClusterNode {
    node_with_lanes(net, name, n, seed, 4)
}

fn node_with_lanes(
    net: &SimNetwork,
    name: &str,
    n: u8,
    seed: Option<u8>,
    lanes: u8,
) -> ClusterNode {
    let mut config = ClusterConfig::new(name, net.backend(addr(n)))
        .foca_config(fast_foca())
        .call_timeout(Duration::from_secs(10))
        .lanes(NonZeroU8::new(lanes).unwrap());
    if let Some(seed) = seed {
        config = config.seed(Seed::new(
            format!("node-{}", (b'a' + seed - 1) as char),
            addr(seed),
        ));
    }
    ClusterNode::new(Supervisor::blueprint().rand_name(), config).with_exit_delay(Duration::ZERO)
}

impl Node {
    fn run(node: ClusterNode) -> Self {
        Self {
            cluster: node.cluster(),
            shutdown: node.shutdown_handle(),
            task: tokio::spawn(node.run()),
        }
    }
}

/// Fails the test if `future` takes unreasonably long in virtual time.
async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(120), future)
        .await
        .expect("Timed out")
}

/// Two nodes that know each other, and the address of an actor on `node-b`
/// as seen from `node-a`.
struct Pair {
    a: Node,
    b: Node,
}

impl Pair {
    async fn start() -> Self {
        Self::start_on(SimNetwork::new(1)).await
    }

    async fn start_on(net: SimNetwork) -> Self {
        Self::start_with_lanes(net, 4, 4).await
    }

    /// With `a_lanes` and `b_lanes` lanes to a peer on node-a and node-b.
    async fn start_with_lanes(net: SimNetwork, a_lanes: u8, b_lanes: u8) -> Self {
        let a = Node::run(node_with_lanes(&net, "node-a", 1, None, a_lanes));
        let b = Node::run(node_with_lanes(&net, "node-b", 2, Some(1), b_lanes));
        within(a.cluster.wait_for_members(1)).await;
        within(b.cluster.wait_for_members(1)).await;

        // What both accept: node-b for receiving, node-a for checking the
        // actors it addresses.
        for node in [&a, &b] {
            node.cluster
                .register::<Double>()
                .register::<Note>()
                .register::<Hang>()
                .register::<Forget>()
                .register::<Slow>()
                .register::<Unwanted>()
                .register::<Blob>()
                .register::<Reverse>()
                .register::<Fetch>()
                .register::<FetchAndForget>()
                .register::<FetchAndHang>()
                .register::<Both>()
                .register::<BigFetch>();
        }
        Self { a, b }
    }

    /// The actor `name` on node-b, as node-a sees it.
    fn on_b<C: zestors::runtime::Context>(&self, name: &'static str) -> RemoteAddress<C> {
        self.a
            .cluster
            .address_unchecked(GlobalName::new(name, "node-b"))
    }
}

#[tokio::test(start_paused = true)]
async fn a_call_gets_the_actors_reply() {
    let pair = Pair::start().await;
    let _worker = worker("remote-call", Log::default());

    let worker = pair.on_b::<WorkerInterface>("remote-call");
    assert_eq!(worker.call(Double(21)).await.unwrap(), 42);
    assert_eq!(worker.call(Double(4)).await.unwrap(), 8);
}

#[tokio::test(start_paused = true)]
async fn a_cast_is_delivered_without_a_reply() {
    let pair = Pair::start().await;
    let log = Log::default();
    let _worker = worker("remote-cast", log.clone());

    pair.on_b::<WorkerInterface>("remote-cast")
        .cast(Note(7))
        .await
        .unwrap();
    within(async {
        while log.notes().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert_eq!(log.notes(), [7]);
}

/// Messages to one actor arrive in the order they were sent.
#[tokio::test(start_paused = true)]
async fn messages_to_one_actor_arrive_in_order() {
    // Messages take very different times to arrive, so pipes overtake each other.
    let net = SimNetwork::new(1);
    net.set_latency(Duration::from_millis(5), Duration::from_millis(50));
    let pair = Pair::start_on(net).await;
    let log = Log::default();
    let _worker = worker("remote-order", log.clone());

    let worker = pair.on_b::<WorkerInterface>("remote-order");
    for i in 0..1_000 {
        worker.cast(Note(i)).await.unwrap();
    }
    // A call is behind all of them, on the same pipe.
    assert_eq!(worker.call(Double(1)).await.unwrap(), 2);
    assert_eq!(log.notes(), (0..1_000).collect::<Vec<_>>());
}

/// However many lanes each node uses, calls are answered and the messages to
/// one actor stay in order. The nodes don't need to agree on the number.
#[tokio::test(start_paused = true)]
async fn the_number_of_lanes_can_be_chosen() {
    for (a_lanes, b_lanes) in [(1, 1), (8, 8), (1, 4), (4, 1), (255, 2)] {
        let net = SimNetwork::new(1);
        net.set_latency(Duration::from_millis(5), Duration::from_millis(50));
        let pair = Pair::start_with_lanes(net, a_lanes, b_lanes).await;
        let names = [
            "lanes-0", "lanes-1", "lanes-2", "lanes-3", "lanes-4", "lanes-5",
        ];
        let logs: Vec<Log> = names.iter().map(|_| Log::default()).collect();
        let _workers: Vec<_> = names
            .iter()
            .zip(&logs)
            .map(|(name, log)| worker(name, log.clone()))
            .collect();

        for name in names {
            let worker = pair.on_b::<WorkerInterface>(name);
            for i in 0..100 {
                worker.cast(Note(i)).await.unwrap();
            }
            assert_eq!(worker.call(Double(3)).await.unwrap(), 6);
        }
        for log in logs {
            assert_eq!(
                log.notes(),
                (0..100).collect::<Vec<_>>(),
                "{a_lanes}/{b_lanes}"
            );
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_message_type_can_be_put_on_the_wire_without_serde() {
    let pair = Pair::start().await;
    let _worker = worker("remote-reverse", Log::default());

    let reversed = pair
        .on_b::<WorkerInterface>("remote-reverse")
        .call(Reverse("stressed".into()))
        .await
        .unwrap();
    assert_eq!(reversed, Reversed("desserts".into()));
}

#[tokio::test(start_paused = true)]
async fn the_reason_a_message_wasnt_delivered_is_told() {
    let pair = Pair::start().await;
    let _worker = worker("remote-reasons", Log::default());

    fn remote_error<T: std::fmt::Debug>(
        result: Result<T, RemoteCallError<impl std::fmt::Debug>>,
    ) -> RemoteError {
        match result {
            Err(RemoteCallError::Reply(RemoteReplyError::Remote(error))) => error,
            other => panic!("Expected an error from the node, got {other:?}"),
        }
    }

    // A message type that node-b doesn't have registered.
    let unknown = pair.on_b::<Dyn<(Unknown,)>>("remote-reasons");
    assert_eq!(
        remote_error(unknown.call(Unknown).await),
        RemoteError::UnknownMessage
    );

    // An actor that isn't there.
    let nobody = pair.on_b::<Dyn<(Double,)>>("remote-nobody");
    assert_eq!(
        remote_error(nobody.call(Double(1)).await),
        RemoteError::NoSuchActor
    );

    // An actor that doesn't accept the message.
    let unwanted = pair.on_b::<Dyn<(Unwanted,)>>("remote-reasons");
    assert_eq!(
        remote_error(unwanted.call(Unwanted).await),
        RemoteError::NotAccepted
    );

    // An actor that drops the request.
    let forgetful = pair.on_b::<WorkerInterface>("remote-reasons");
    assert_eq!(
        remote_error(forgetful.call(Forget).await),
        RemoteError::NoReply
    );
}

#[tokio::test(start_paused = true)]
async fn a_call_that_is_not_answered_times_out() {
    let pair = Pair::start().await;
    let _worker = worker("remote-timeout", Log::default());

    let hangs = pair
        .on_b::<WorkerInterface>("remote-timeout")
        .with_timeout(Duration::from_secs(2));
    let started = tokio::time::Instant::now();
    assert!(matches!(
        hangs.call(Hang).await,
        Err(RemoteCallError::Reply(RemoteReplyError::Timeout))
    ));
    assert_eq!(started.elapsed(), Duration::from_secs(2));
}

#[tokio::test(start_paused = true)]
async fn a_call_fails_when_the_node_leaves() {
    let pair = Pair::start().await;
    let _worker = worker("remote-leave", Log::default());

    let reply = pair
        .on_b::<WorkerInterface>("remote-leave")
        .cast(Hang)
        .await
        .unwrap();
    pair.b.shutdown.shutdown();
    assert!(matches!(
        within(reply.wait()).await,
        Err(RemoteReplyError::Disconnected)
    ));
}

#[tokio::test(start_paused = true)]
async fn a_call_fails_when_the_node_crashes() {
    let pair = Pair::start().await;
    let _worker = worker("remote-crash", Log::default());

    let reply = pair
        .on_b::<WorkerInterface>("remote-crash")
        .cast(Hang)
        .await
        .unwrap();
    // No goodbye: the node is just gone.
    pair.b.task.abort();
    assert!(matches!(
        within(reply.wait()).await,
        Err(RemoteReplyError::Disconnected)
    ));
}

#[tokio::test(start_paused = true)]
async fn messages_that_cant_be_sent_are_given_back() {
    let pair = Pair::start().await;

    // A node that isn't in the cluster.
    let stranger: RemoteAddress<Dyn<(Note,)>> = pair
        .a
        .cluster
        .address_unchecked(GlobalName::new("any", "node-z"));
    let Err(RemoteCastError::NotAMember(Note(3))) = stranger.cast(Note(3)).await else {
        panic!("Expected the message back")
    };

    // Too large for the network.
    let blobs = pair.on_b::<Dyn<(Blob,)>>("any");
    let Err(RemoteCastError::TooLarge { msg, size, max }) =
        blobs.cast(Blob(vec![0; 5 * 1024 * 1024])).await
    else {
        panic!("Expected the message back")
    };
    assert_eq!(msg.0.len(), 5 * 1024 * 1024);
    assert!(size > max);
}

#[tokio::test(start_paused = true)]
async fn nothing_is_sent_before_the_node_runs() {
    let net = SimNetwork::new(1);
    let node = node(&net, "node-a", 1, None);
    let remote = node.cluster();

    let target: RemoteAddress<Dyn<(Note,)>> =
        remote.address_unchecked(GlobalName::new("any", "node-b"));
    assert!(matches!(
        target.cast(Note(1)).await,
        Err(RemoteCastError::NotRunning(_))
    ));
}

/// One slow actor doesn't hold up the others on the same node.
#[tokio::test(start_paused = true)]
async fn a_slow_actor_does_not_delay_another() {
    let pair = Pair::start().await;
    let _slow = worker("remote-slow", Log::default());
    let _fast = worker("remote-fast", Log::default());

    let slow = pair.on_b::<WorkerInterface>("remote-slow");
    let fast = pair.on_b::<WorkerInterface>("remote-fast");

    // The slow actor has a queue of slow work.
    let mut slow_replies = Vec::new();
    for _ in 0..5 {
        slow_replies.push(slow.cast(Slow(1_000)).await.unwrap());
    }

    let started = tokio::time::Instant::now();
    assert_eq!(fast.call(Double(2)).await.unwrap(), 4);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "The fast actor waited for the slow one: {:?}",
        started.elapsed()
    );

    for reply in slow_replies {
        assert_eq!(within(reply.wait()).await.unwrap(), 1);
    }
}

/// Code that is generic over what it sends to takes any address that accepts the message.
async fn double_via(target: &impl RemoteAccepts<Double>, n: u32) -> u32 {
    target.call(Double(n)).await.unwrap()
}

#[tokio::test(start_paused = true)]
async fn generic_code_can_send_through_the_trait() {
    let pair = Pair::start().await;
    let _worker = worker("remote-generic", Log::default());

    // By interface, and by the one message it is used for.
    let by_interface = pair.on_b::<WorkerInterface>("remote-generic");
    let by_message = pair.on_b::<Dyn<(Double,)>>("remote-generic");
    assert_eq!(double_via(&by_interface, 5).await, 10);
    assert_eq!(double_via(&by_message, 6).await, 12);
}

#[tokio::test(start_paused = true)]
async fn a_call_can_have_a_timeout_of_its_own() {
    let pair = Pair::start().await;
    let _worker = worker("remote-call-options", Log::default());

    // The address would wait 10 seconds; this call doesn't.
    let hangs = pair.on_b::<WorkerInterface>("remote-call-options");
    let started = tokio::time::Instant::now();
    let result = hangs
        .call_with(
            Hang,
            RemoteCallOptions::new().timeout(Duration::from_secs(1)),
        )
        .await;
    assert!(matches!(
        result,
        Err(RemoteCallError::Reply(RemoteReplyError::Timeout))
    ));
    assert_eq!(started.elapsed(), Duration::from_secs(1));
}

/// Sending returns what the message gives back to wait on, like a local cast:
/// nothing for a message that expects no reply, and the reply for one that does.
#[tokio::test(start_paused = true)]
async fn casting_gives_the_messages_receipt() {
    let pair = Pair::start().await;
    let log = Log::default();
    let _worker = worker("remote-receipts", log.clone());
    let worker = pair.on_b::<WorkerInterface>("remote-receipts");

    // No reply expected: `()`, as soon as it is queued.
    let (): () = worker.cast(Note(1)).await.unwrap();
    worker.try_cast(Note(2)).unwrap();

    // A reply expected: something to wait for it with, while doing other things.
    let first = worker.cast(Double(10)).await.unwrap();
    let second = worker.try_cast(Double(20)).unwrap();
    assert_eq!(second.wait().await.unwrap(), 40);
    assert_eq!(first.wait().await.unwrap(), 20);
    assert_eq!(log.notes(), [1, 2]);
}

// Requests inside messages.

#[tokio::test(start_paused = true)]
async fn a_request_in_a_message_is_answered_across_nodes() {
    let pair = Pair::start().await;
    let _worker = worker("request-answer", Log::default());
    let worker = pair.on_b::<WorkerInterface>("request-answer");

    for n in [1, 41] {
        let (reply, answer) = RemoteRequest::new();
        worker.cast(Fetch { n, reply }).await.unwrap();
        assert_eq!(within(answer).await.unwrap(), n + 1);
    }
}

#[tokio::test(start_paused = true)]
async fn a_call_can_carry_a_request_as_well() {
    let pair = Pair::start().await;
    let _worker = worker("request-both", Log::default());

    let (reply, answer) = RemoteRequest::new();
    let call = pair
        .on_b::<WorkerInterface>("request-both")
        .call(Both { n: 5, reply })
        .await
        .unwrap();
    assert_eq!(call, 5);
    assert_eq!(within(answer).await.unwrap(), 105);
}

#[tokio::test(start_paused = true)]
async fn a_request_the_actor_drops_fails_its_reply() {
    let pair = Pair::start().await;
    let _worker = worker("request-drop", Log::default());

    let (reply, answer) = RemoteRequest::new();
    pair.on_b::<WorkerInterface>("request-drop")
        .cast(FetchAndForget { reply })
        .await
        .unwrap();
    assert!(within(answer).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn a_request_for_an_actor_that_is_not_there_fails_its_reply() {
    let pair = Pair::start().await;

    let (reply, answer) = RemoteRequest::new();
    pair.on_b::<WorkerInterface>("request-nobody")
        .cast(FetchAndForget { reply })
        .await
        .unwrap();
    assert!(within(answer).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn a_request_fails_when_the_node_holding_it_is_lost() {
    let pair = Pair::start().await;
    let _worker = worker("request-lost", Log::default());

    let (reply, answer) = RemoteRequest::new();
    pair.on_b::<WorkerInterface>("request-lost")
        .cast(FetchAndHang { reply })
        .await
        .unwrap();
    // It is with the actor now, and waits there until the node goes away.
    tokio::time::sleep(Duration::from_secs(5)).await;
    pair.b.task.abort();
    assert!(within(answer).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn a_request_in_a_message_that_is_not_sent_fails_its_reply() {
    let pair = Pair::start().await;

    let (reply, answer) = RemoteRequest::new();
    let Err(RemoteCastError::TooLarge { .. }) = pair
        .on_b::<Dyn<(BigFetch,)>>("any")
        .cast(BigFetch {
            blob: vec![0; 5 * 1024 * 1024],
            reply,
        })
        .await
    else {
        panic!("Expected the message back")
    };
    assert!(within(answer).await.is_err());
}

#[test]
fn a_request_is_only_serialized_as_part_of_a_sent_message() {
    let (request, _reply) = RemoteRequest::<u32>::new();
    assert!(postcard::to_allocvec(&request).is_err());
}

/// Signals sent to an actor on another node work as they do locally.
#[tokio::test(start_paused = true)]
async fn an_actor_on_another_node_can_be_signalled() {
    let pair = Pair::start().await;
    let local = worker("ops-signal", Log::default());
    local.watch_init().await.unwrap();
    let remote = pair.on_b::<WorkerInterface>("ops-signal");

    assert_eq!(remote.status().await.unwrap(), ActorStatus::Running);

    assert!(remote.signal_suspend().await.unwrap());
    remote.ping().await.unwrap();
    assert_eq!(remote.status().await.unwrap(), ActorStatus::Suspended);

    assert!(remote.signal_resume().await.unwrap());
    remote.ping().await.unwrap();
    assert_eq!(remote.status().await.unwrap(), ActorStatus::Running);

    assert!(remote.signal_shutdown().await.unwrap());
    assert!(within(local.watch_exit()).await.is_ok());
    assert!(remote.is_dead().await.unwrap());
    assert!(!remote.signal_shutdown().await.unwrap(), "Already dead");
}

/// A signal isn't stuck behind the messages queued for a busy actor. Delivering
/// each of them to a mailbox that is filling up is slowed down, so many
/// queued messages take a long time to get through.
#[tokio::test(start_paused = true)]
async fn a_signal_does_not_wait_for_the_actors_messages() {
    let pair = Pair::start().await;
    let _worker = worker("ops-busy", Log::default());
    let remote = pair.on_b::<WorkerInterface>("ops-busy");

    let _busy = remote.cast(Slow(600_000)).await.unwrap();
    for i in 0..2_000 {
        remote.cast(Note(i)).await.unwrap();
    }

    let started = tokio::time::Instant::now();
    assert!(remote.signal_shutdown().await.unwrap());
    let info = remote.info().await.unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "The signal waited for the actor: {:?}",
        started.elapsed()
    );
    assert!(info.snapshot.msg_len > 0, "{:?}", info.snapshot);
    assert!(remote.msg_len().await.unwrap() > 0);
}

#[tokio::test(start_paused = true)]
async fn what_an_actor_accepts_can_be_asked() {
    let pair = Pair::start().await;
    let _worker = worker("ops-accepts", Log::default());
    let remote = pair.on_b::<WorkerInterface>("ops-accepts");

    assert!(remote.accepts::<Double>().await.unwrap());
    assert!(remote.accepts::<Note>().await.unwrap());
    // Registered on the node, but not accepted by the actor.
    assert!(!remote.accepts::<Unwanted>().await.unwrap());
    // Not registered on the node.
    assert!(!remote.accepts::<Unknown>().await.unwrap());

    let members = remote.members().await.unwrap();
    assert!(members.contains(&Double::Id) && members.contains(&Note::Id));
    assert!(!members.contains(&Unwanted::Id));
    assert!(
        remote
            .is_superset_of(&[Double::Id, Note::Id])
            .await
            .unwrap()
    );
    assert!(
        !remote
            .is_superset_of(&[Double::Id, Unwanted::Id])
            .await
            .unwrap()
    );

    assert!(!remote.reached_backpressure().await.unwrap());
    assert!(remote.msg_is_empty().await.unwrap());
    assert!(remote.signal_is_empty().await.unwrap());
    assert!(!remote.is_exiting().await.unwrap());
    let snapshot = remote.snapshot().await.unwrap();
    assert_eq!(&snapshot.name, remote.name().name());
    assert!(remote.last_spawned_at().await.unwrap().is_some());
    assert_eq!(remote.name().node().to_string(), "node-b");
}

#[tokio::test(start_paused = true)]
async fn operations_report_why_they_failed() {
    let pair = Pair::start().await;

    let nobody = pair.on_b::<WorkerInterface>("ops-nobody");
    assert!(matches!(
        nobody.ping().await,
        Err(RemoteOpError::Reply(RemoteReplyError::Remote(
            RemoteError::NoSuchActor
        )))
    ));
    assert!(matches!(
        nobody.info().await,
        Err(RemoteOpError::Reply(RemoteReplyError::Remote(
            RemoteError::NoSuchActor
        )))
    ));

    let stranger: RemoteAddress<WorkerInterface> = pair
        .a
        .cluster
        .address_unchecked(GlobalName::new("ops-nobody", "node-z"));
    assert!(matches!(
        stranger.signal_shutdown().await,
        Err(RemoteOpError::NotSent(RemoteCastError::NotAMember(())))
    ));
}

/// The address of an actor on node-b, as node-b itself sees it: a local one.
async fn local_on_b(pair: &Pair, name: &'static str) -> ClusterAddress<WorkerInterface> {
    let address = pair
        .b
        .cluster
        .address::<WorkerInterface>(GlobalName::new(name, "node-b"))
        .await
        .unwrap();
    assert!(matches!(address, ClusterAddress::Local(_)));
    address
}

/// The same actor as seen from node-a: a remote one.
async fn remote_on_b(pair: &Pair, name: &'static str) -> ClusterAddress<WorkerInterface> {
    let address = pair
        .a
        .cluster
        .address::<WorkerInterface>(GlobalName::new(name, "node-b"))
        .await
        .unwrap();
    assert!(matches!(address, ClusterAddress::Remote(_)));
    address
}

#[tokio::test(start_paused = true)]
async fn a_cluster_address_reaches_an_actor_on_this_node() {
    let pair = Pair::start().await;
    let log = Log::default();
    let _worker = worker("cluster-local", log.clone());
    let local = local_on_b(&pair, "cluster-local").await;

    assert_eq!(local.call(Double(21)).await.unwrap(), 42);
    for i in 0..100 {
        local.cast(Note(i)).await.unwrap();
    }
    // A call is behind all of them.
    assert_eq!(local.call(Double(1)).await.unwrap(), 2);
    assert_eq!(log.notes(), (0..100).collect::<Vec<_>>());

    // A cast has a receipt to wait on for the reply.
    assert_eq!(
        local.cast(Double(4)).await.unwrap().wait().await.unwrap(),
        8
    );
    assert_eq!(local.try_cast(Double(5)).unwrap().wait().await.unwrap(), 10);

    // A reply channel in the message is just a request.
    let (reply, count) = RemoteRequest::new();
    local.cast(Fetch { n: 1, reply }).await.unwrap();
    assert_eq!(count.await.unwrap(), 2);
}

/// The same code takes an actor wherever it is.
#[tokio::test(start_paused = true)]
async fn the_same_code_works_for_every_kind_of_address() {
    let pair = Pair::start().await;
    let _worker = worker("cluster-generic", Log::default());

    assert_eq!(
        double_via(&local_on_b(&pair, "cluster-generic").await, 3).await,
        6
    );
    assert_eq!(
        double_via(&remote_on_b(&pair, "cluster-generic").await, 4).await,
        8
    );
    assert_eq!(
        double_via(&pair.on_b::<WorkerInterface>("cluster-generic"), 5).await,
        10
    );
}

/// A message to an actor on this node isn't encoded, or it couldn't be sent
/// here.
#[tokio::test(start_paused = true)]
async fn a_local_message_is_not_encoded() {
    let pair = Pair::start().await;
    let _worker = worker("cluster-nowire", Log::default());

    assert_eq!(
        local_on_b(&pair, "cluster-nowire")
            .await
            .call(NoWire(1))
            .await
            .unwrap(),
        2
    );
    assert!(matches!(
        remote_on_b(&pair, "cluster-nowire")
            .await
            .call(NoWire(1))
            .await,
        Err(RemoteCallError::NotSent(RemoteCastError::Encode { .. }))
    ));
}

#[tokio::test(start_paused = true)]
async fn a_local_actor_is_reached_without_the_node_running() {
    let net = SimNetwork::new(1);
    let idle = node(&net, "node-x", 9, None);
    let remote = idle.cluster();
    let _worker = worker("cluster-idle", Log::default());

    let local = remote
        .address::<WorkerInterface>(GlobalName::new("cluster-idle", "node-x"))
        .await
        .unwrap();
    assert_eq!(local.call(Double(2)).await.unwrap(), 4);

    // Another node has to be asked, which a node that doesn't run can't do.
    assert!(matches!(
        remote
            .address::<WorkerInterface>(GlobalName::new("cluster-idle", "node-y"))
            .await,
        Err(AddressError::Remote(RemoteOpError::NotSent(
            RemoteCastError::NotRunning(_)
        )))
    ));
    let elsewhere = ClusterAddress::from(
        remote.address_unchecked::<WorkerInterface>(GlobalName::new("cluster-idle", "node-y")),
    );
    assert!(matches!(
        elsewhere.call(Double(2)).await,
        Err(RemoteCallError::NotSent(RemoteCastError::NotRunning(_)))
    ));
}

#[tokio::test(start_paused = true)]
async fn a_local_call_can_time_out_and_a_closed_actor_is_told() {
    let pair = Pair::start().await;
    let local_worker = worker("cluster-timeout", Log::default());
    let local = local_on_b(&pair, "cluster-timeout").await;

    // No timeout of its own, but a call can have one.
    let started = tokio::time::Instant::now();
    let result = local
        .call_with(
            Hang,
            RemoteCallOptions::new().timeout(Duration::from_secs(2)),
        )
        .await;
    assert!(matches!(
        result,
        Err(RemoteCallError::Reply(RemoteReplyError::Timeout))
    ));
    assert_eq!(started.elapsed(), Duration::from_secs(2));

    // An actor that drops the request.
    assert!(matches!(
        local.call(Forget).await,
        Err(RemoteCallError::Reply(RemoteReplyError::Remote(
            RemoteError::NoReply
        )))
    ));

    local_worker.signal_shutdown();
    within(local_worker.watch_exit()).await.unwrap();
    assert!(matches!(
        local.call(Double(1)).await,
        Err(RemoteCallError::NotSent(RemoteCastError::Closed(_)))
    ));
    assert!(matches!(
        local.try_cast(Double(1)),
        Err(RemoteCastError::Closed(_))
    ));
}

#[tokio::test(start_paused = true)]
async fn making_an_address_checks_the_actor_where_it_is() {
    let pair = Pair::start().await;
    let _worker = worker("cluster-check", Log::default());
    let (a, b) = (&pair.a.cluster, &pair.b.cluster);

    // On this node the actor is looked for in its registry.
    assert!(matches!(
        b.address::<WorkerInterface>(GlobalName::new("cluster-nobody", "node-b"))
            .await,
        Err(AddressError::NoSuchActor(_))
    ));
    assert!(matches!(
        b.address_dyn::<(Unwanted,)>(GlobalName::new("cluster-check", "node-b"))
            .await,
        Err(AddressError::TypeMismatch(_))
    ));
    assert!(
        b.address_dyn::<(Double, Note)>(GlobalName::new("cluster-check", "node-b"))
            .await
            .is_ok()
    );

    // On another node, that node is asked.
    assert!(matches!(
        a.address::<WorkerInterface>(GlobalName::new("cluster-nobody", "node-b"))
            .await,
        Err(AddressError::NoSuchActor(_))
    ));
    assert!(matches!(
        a.address_dyn::<(Unwanted,)>(GlobalName::new("cluster-check", "node-b"))
            .await,
        Err(AddressError::TypeMismatch(_))
    ));
    assert!(matches!(
        a.address::<WorkerInterface>(GlobalName::new("cluster-check", "node-b"))
            .await,
        Ok(ClusterAddress::Remote(_))
    ));
    assert!(matches!(
        a.address_dyn::<(Double, Note)>(GlobalName::new("cluster-check", "node-b"))
            .await,
        Ok(ClusterAddress::Remote(_))
    ));
    // A node that isn't a member can't be asked.
    assert!(matches!(
        a.address::<WorkerInterface>(GlobalName::new("cluster-check", "node-z"))
            .await,
        Err(AddressError::Remote(RemoteOpError::NotSent(
            RemoteCastError::NotAMember(_)
        )))
    ));
}

#[tokio::test(start_paused = true)]
async fn an_address_can_be_made_unchecked() {
    let pair = Pair::start().await;
    let (a, b) = (&pair.a.cluster, &pair.b.cluster);

    // Conversions.
    let remote = a.address_unchecked::<WorkerInterface>(GlobalName::new("cluster-check", "node-b"));
    assert!(matches!(
        ClusterAddress::from(remote),
        ClusterAddress::Remote(_)
    ));

    // A remote address to this node's own actor still goes over the network,
    // where this node isn't a member.
    let itself = b.address_unchecked::<WorkerInterface>(GlobalName::new("cluster-check", "node-b"));
    assert!(matches!(
        itself.call(Double(1)).await,
        Err(RemoteCallError::NotSent(RemoteCastError::NotAMember(_)))
    ));
}

#[tokio::test(start_paused = true)]
async fn a_local_actor_can_be_operated_on_through_a_cluster_address() {
    let pair = Pair::start().await;
    let child = worker("cluster-ops", Log::default());
    child.watch_init().await.unwrap();
    let local = local_on_b(&pair, "cluster-ops").await;

    assert_eq!(local.status().await.unwrap(), ActorStatus::Running);
    local.ping().await.unwrap();
    assert!(local.msg_is_empty().await.unwrap());
    assert!(!local.reached_backpressure().await.unwrap());
    assert_eq!(&local.snapshot().await.unwrap().name, local.name());
    assert!(local.last_spawned_at().await.unwrap().is_some());
    assert!(local.accepts::<Double>().await.unwrap());
    assert!(!local.accepts::<Unwanted>().await.unwrap());

    // What needs the node's registry of messages isn't known for a local actor.
    assert!(matches!(
        local.members().await,
        Err(RemoteOpError::Unsupported)
    ));
    assert!(matches!(
        local.accepts_id(Double::Id).await,
        Err(RemoteOpError::Unsupported)
    ));
    assert!(local.info().await.unwrap().accepts.is_none());

    assert!(local.signal_suspend().await.unwrap());
    local.ping().await.unwrap();
    assert_eq!(local.status().await.unwrap(), ActorStatus::Suspended);
    assert!(local.signal_resume().await.unwrap());
    local.ping().await.unwrap();
    assert!(local.signal_shutdown().await.unwrap());
    assert!(within(child.watch_exit()).await.is_ok());
    assert!(local.is_dead().await.unwrap());

    // The same for the actor seen from the other node, over the network.
    let _other = worker("cluster-ops-remote", Log::default());
    let remote = remote_on_b(&pair, "cluster-ops-remote").await;
    assert!(remote.accepts::<Double>().await.unwrap());
    assert!(remote.members().await.unwrap().contains(&Double::Id));
    assert!(remote.info().await.unwrap().accepts.is_some());
}

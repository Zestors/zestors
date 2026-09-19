//! Messages to actors on other nodes: two cluster nodes over the simulated
//! network, on virtual time.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::task::JoinHandle;
use zestors::{
    interface::{Envelope, Interface, Message},
    prelude::*,
    runtime::{Dyn, Inbox, Name, spawn},
    supervisor::Supervisor,
};
use zestors_distr::{
    Cluster, ClusterConfig, ClusterNode, ClusterNodeError, Decode, DecodeError, Encode,
    EncodeError, GlobalName, Remote, RemoteAccepts, RemoteAddress, RemoteCallError,
    RemoteCallOptions, RemoteCastError, RemoteError, RemoteReceipt as _, RemoteReplyError,
    RemoteRequest, Seed, StableId, sim::SimNetwork,
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

#[derive(Interface, Debug)]
enum WorkerInterface {
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
    remote: Remote,
    cluster: Cluster,
    shutdown: zestors_supervisor::NodeShutdown,
    task: JoinHandle<Result<(), ClusterNodeError>>,
}

fn node(net: &SimNetwork, name: &str, n: u8, seed: Option<u8>) -> ClusterNode {
    let mut config = ClusterConfig::new(name, net.backend(addr(n)))
        .foca_config(fast_foca())
        .call_timeout(Duration::from_secs(10));
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
            remote: node.remote(),
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
        let a = Node::run(node(&net, "node-a", 1, None));
        let b = Node::run(node(&net, "node-b", 2, Some(1)));
        within(a.cluster.wait_for_members(1)).await;
        within(b.cluster.wait_for_members(1)).await;

        // What node-b accepts.
        b.remote
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
        Self { a, b }
    }

    /// The actor `name` on node-b, as node-a sees it.
    fn on_b<C: zestors::runtime::Context>(&self, name: &'static str) -> RemoteAddress<C> {
        self.a.remote.address(GlobalName::new(name, "node-b"))
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
    let stranger: RemoteAddress<Dyn<(Note,)>> =
        pair.a.remote.address(GlobalName::new("any", "node-z"));
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
    let remote = node.remote();

    let target: RemoteAddress<Dyn<(Note,)>> = remote.address(GlobalName::new("any", "node-b"));
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

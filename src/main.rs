use std::env::{self, Args};
use std::fmt;
use std::pin::Pin;
use std::str::FromStr;
use std::time::Duration;

use futures::{future, join, stream, Future, StreamExt};
use tokio::process::Command;
use uuid::Uuid;
use zestors::actor::{self};
use zestors::address::{Address, Addressable};
use zestors::context::{BasicContext, NoCtx};
use zestors::distributed::node::Node;
use zestors::distributed::pid::ProcessRef;
use zestors::distributed::NodeId;
use zestors::flows::{InitFlow, ReqFlow};
use zestors::{
    actor::Actor,
    distributed::local_node::{self, LocalNode},
    flows::MsgFlow,
};

#[tokio::main]
pub async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() == 1 {
        let token1 = Uuid::new_v4();
        let token2 = Uuid::new_v4();

        let task1 = tokio::task::spawn(async move {
            let mut child1 = Command::new("cargo")
                .arg("run")
                .arg(token1.to_string())
                .arg("1")
                .spawn()
                .unwrap();

            child1.wait().await
        });

        let task2 = tokio::task::spawn(async move {
            let mut child2 = Command::new("cargo")
                .arg("run")
                .arg(token1.to_string())
                .arg("2")
                .spawn()
                .unwrap();
            child2.wait().await
        });

        let task3 = tokio::task::spawn(async move {
            let mut child3 = Command::new("cargo")
                .arg("run")
                .arg(token1.to_string())
                .arg("3")
                .spawn()
                .unwrap();
            child3.wait().await
        });

        let res = join!(task1, task2, task3);
    } else if args.len() == 3 {
        let token = Uuid::from_str(args.get(1).unwrap()).unwrap();
        let id = NodeId::from_str(args.get(2).unwrap()).unwrap();
        let socket_addr = format!("127.0.0.1:123{}", id).parse().unwrap();

        local_node::initialize(token, id, socket_addr)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        match id {
            1 => main1().await,
            2 => main2().await,
            3 => main3().await,
            _ => panic!(),
        }

        future::pending::<()>().await;
    } else {
        panic!()
    }
}

async fn main1() {
    let local_node = local_node::get();
    let registry = local_node.registry();
    let cluster = local_node.cluster();
    let node2 = local_node
        .connect("127.0.0.1:1232".parse().unwrap())
        .await
        .unwrap();
}

async fn main2() {
    let local_node = local_node::get();
    let registry = local_node.registry();
    let node3 = local_node
        .connect("127.0.0.1:1233".parse().unwrap())
        .await
        .unwrap();

    let mut child = actor::spawn::<MyActor>(MyActor);
    child.detach();

    registry
        .register(child.address(), Some("my_actor".to_string()))
        .unwrap();

    println!("spawned child: {:?}", child)
}

async fn main3() {
    let local_node = local_node::get();
    let registry = local_node.registry();
    let node1 = local_node
        .connect("127.0.0.1:1231".parse().unwrap())
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    let node2 = local_node.cluster().get_node(2).unwrap();

    let process_ref = node2
        .find_process_by_name::<MyActor2>("my_actor".to_string())
        .await;
    println!("ref: {:?}", process_ref);

    let process_ref = node2
        .find_process_by_name::<MyActor>("my_actor".to_string())
        .await;
    println!("ref: {:?}", process_ref);

    let process_ref = node1
        .find_process_by_name::<MyActor>("my_actor".to_string())
        .await;
    println!("ref: {:?}", process_ref);

    let process_ref = node2
        .find_process_by_name::<MyActor>("my_actor2".to_string())
        .await;
    println!("ref: {:?}", process_ref);
}

type BoxFut<T> = Pin<Box<dyn Future<Output = T>>>;

trait CallRemote<'a, A: Actor, M, P, F, R: 'a> {
    fn call_rem(&'a self, function: fn(&mut A, &mut A::Context, M) -> F, params: P) -> R;
}

#[derive(Debug)]
struct MyActor;

#[async_trait::async_trait]
impl Actor for MyActor {
    type Init = Self;
    type ExitError = zestors::AnyhowError;
    type ExitNormal = ();
    type ExitWith = actor::ExitReason<Self>;
    type Context = NoCtx<Self>;
    const ABORT_TIMER: Duration = Duration::from_secs(5);

    async fn init(init: Self::Init, address: Address<Self>) -> InitFlow<Self> {
        InitFlow::Init(init)
    }

    async fn exit(self, reason: actor::ExitReason<Self>) -> Self::ExitWith {
        reason
    }

    fn ctx(&mut self) -> &mut Self::Context {
        NoCtx::new()
    }
}

impl MyActor {
    pub async fn my_message(
        &mut self,
        _: &mut BasicContext<Self>,
        params: (u32, u32),
    ) -> MsgFlow<Self> {
        MsgFlow::Ok
    }

    pub async fn say_hello(&mut self, _: &mut BasicContext<Self>, params: u32) -> MsgFlow<Self> {
        MsgFlow::Ok
    }
}

#[derive(Debug)]
struct MyActor2;

#[async_trait::async_trait]
impl Actor for MyActor2 {
    type Init = Self;
    type ExitError = zestors::AnyhowError;
    type ExitNormal = ();
    type ExitWith = actor::ExitReason<Self>;
    type Context = NoCtx<Self>;
    const ABORT_TIMER: Duration = Duration::from_secs(5);

    async fn init(init: Self::Init, address: Address<Self>) -> InitFlow<Self> {
        InitFlow::Init(init)
    }

    async fn exit(self, reason: actor::ExitReason<Self>) -> Self::ExitWith {
        reason
    }

    fn ctx(&mut self) -> &mut Self::Context {
        NoCtx::new()
    }
}

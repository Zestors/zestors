use std::env::{self, Args};
use std::fmt::{self};
use std::marker::PhantomData;
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
use zestors::errors::{DidntArrive, NoReply};
use zestors::flows::{InitFlow, ReqFlow};
use zestors::function::{RefSafe, ReqFn};
use zestors::messaging::Reply;
use zestors::Fn;
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
        .register(child.addr(), Some("my_actor".to_string()))
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

    pub async fn say_hello2(&mut self, _: &str) -> ReqFlow<Self, u32> {
        ReqFlow::Reply(10)
    }

    pub async fn say_hello3<'a>(&'a mut self, val: &'a u32) -> ReqFlow<Self, &u32> {
        ReqFlow::Reply(val)
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

struct Scope<'scope, R>(Vec<&'scope Reply<R>>);

impl<'scope, R> Scope<'scope, R>
where
    R: 'scope,
{
    fn send<A, P>(
        &mut self,
        address: &Address<A>,
        req_fn: ReqFn<A, P, R, RefSafe>,
        params: P,
    ) -> Result<(), DidntArrive<P>>
    where
        P: 'scope,
    {
        todo!()
    }

    fn send_map<A, P, R2>(
        &mut self,
        address: &Address<A>,
        req_fn: ReqFn<A, P, R2, RefSafe>,
        params: P,
        map: impl FnOnce(R2) -> R,
    ) -> Result<(), DidntArrive<P>>
    where
        P: 'scope,
        R2: 'scope,
    {
        todo!()
    }
}

async unsafe fn scope<'a, R>(function: impl FnOnce(&mut Scope<'a, R>)) -> Vec<Result<R, NoReply>> {
    todo!()
}

macro_rules! scope {
    ($function:expr) => {
        unsafe { scope($function).await }
    };
}

async fn test2(address: Address<MyActor>) {
    let str = "hi".to_string();
    let u32 = 10;

    let res = scope!(|scope| {
        scope
            .send(&address, Fn!(MyActor::say_hello2), &str)
            .unwrap();
        scope
            .send(&address, Fn!(MyActor::say_hello2), &str)
            .unwrap();
        scope
            .send(&address, Fn!(MyActor::say_hello2), &str)
            .unwrap();
        scope
            .send_map(&address, Fn!(MyActor::say_hello3), &u32, |val| *val)
            .unwrap();
    });

    drop(str);
}

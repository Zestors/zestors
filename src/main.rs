use std::env::{self, Args};
use std::fmt;
use std::pin::Pin;
use std::str::FromStr;
use std::time::Duration;

use futures::{Future, stream, StreamExt, join};
use tokio::process::Command;
use uuid::Uuid;
use zestors::actor::{self};
use zestors::address::{Address, Addressable};
use zestors::context::{BasicCtx, NoCtx};
use zestors::distributed::NodeId;
use zestors::distributed::node::Node;
use zestors::flows::{InitFlow, ReqFlow};
use zestors::messaging::Req;
use zestors::{
    actor::Actor,
    distributed::local_node::{self, LocalNode},
    flows::MsgFlow,
    messaging::Msg,
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
            _ => panic!()
        }

        tokio::time::sleep(Duration::from_millis(1_000)).await;

    } else {
        panic!()
    }
}

async fn main1() {
    let local_node = local_node::get();
    let registry = local_node.registry();
    let cluster = local_node.cluster();
    let node2 = local_node.connect("127.0.0.1:1232".parse().unwrap()).await.unwrap();
    // let node3 = local_node.connect("127.0.0.1:1233".parse().unwrap()).await.unwrap();


}

async fn main2() {
    let local_node = local_node::get();
    let registry = local_node.registry();
    // let node1 = local_node.connect("127.0.0.1:1231".parse().unwrap()).await.unwrap();
    let node3 = local_node.connect("127.0.0.1:1233".parse().unwrap()).await.unwrap();
}

async fn main3() {
    let local_node = local_node::get();
    let registry = local_node.registry();
    let node1 = local_node.connect("127.0.0.1:1231".parse().unwrap()).await.unwrap();
    // let node2 = local_node.connect("127.0.0.1:1232".parse().unwrap()).await.unwrap();
}

async fn actual_main() {

    let nodes: Vec<Node> = stream::iter(1..4).filter_map(|id| async move {
        if local_node::get().node_id() != id {
            let socket_addr = format!("127.0.0.1:123{}", id).parse().unwrap();
            Some(local_node::get().connect(socket_addr).await.unwrap())
        } else {
            None
        }
    }).collect().await;

    println!("Connected nodes: {:?}", nodes);

    for node in nodes {
        match node.find_process_by_name::<MyActor>("my_actor") {
            Ok(reply) => match reply.await {
                Ok(process) => println!("found process! {:?}", process),
                Err(_) => println!("process not found...")
            },
            Err(_) => println!("node disconnected..."),
        }
    }

    tokio::time::sleep(Duration::from_millis(200)).await;
}


type BoxFut<T> = Pin<Box<dyn Future<Output = T>>>;

trait CallRemote<'a, A: Actor, M, P, F, R: 'a> {
    fn call_rem(&'a self, function: fn(&mut A, &mut A::Ctx, M) -> F, params: P) -> R;
}

mod call_pid {
    use super::*;

    impl<'a, A: Actor, M1> CallRemote<'a, A, M1, &M1, MsgFlow<A>, Msg<'a, A, M1>> for Address<A>
    where
        M1: 'static,
    {
        fn call_rem(
            &self,
            function: fn(&mut A, &mut <A as Actor>::Ctx, M1) -> MsgFlow<A>,
            params: &M1,
        ) -> Msg<'a, A, M1> {
            todo!()
        }
    }

    impl<'a, A: Actor, M1> CallRemote<'a, A, M1, &M1, BoxFut<MsgFlow<A>>, Msg<'a, A, M1>> for Address<A>
    where
        M1: 'static,
    {
        fn call_rem(
            &self,
            function: fn(&mut A, &mut <A as Actor>::Ctx, M1) -> BoxFut<MsgFlow<A>>,
            params: &M1,
        ) -> Msg<'a, A, M1> {
            todo!()
        }
    }

    impl<'a, A: Actor, M1, M2>
        CallRemote<'a, A, (M1, M2), (&M1, &M2), MsgFlow<A>, Msg<'a, A, (M1, M2)>> for Address<A>
    where
        M1: 'static,
        M2: 'static,
    {
        fn call_rem(
            &self,
            function: fn(&mut A, &mut <A as Actor>::Ctx, (M1, M2)) -> MsgFlow<A>,
            params: (&M1, &M2),
        ) -> Msg<'a, A, (M1, M2)> {
            todo!()
        }
    }

    impl<'a, A: Actor, M1, M2>
        CallRemote<'a, A, (M1, M2), (&M1, &M2), BoxFut<MsgFlow<A>>, Msg<'a, A, (M1, M2)>>
        for Address<A>
    where
        M1: 'static,
        M2: 'static,
    {
        fn call_rem(
            &self,
            function: fn(&mut A, &mut <A as Actor>::Ctx, (M1, M2)) -> BoxFut<MsgFlow<A>>,
            params: (&M1, &M2),
        ) -> Msg<'a, A, (M1, M2)> {
            todo!()
        }
    }

    impl<'a, A: Actor, R, M1> CallRemote<'a, A, M1, &M1, ReqFlow<A, R>, Req<'a, A, M1, R>>
        for Address<A>
    where
        R: 'static,
        M1: 'static,
    {
        fn call_rem(
            &self,
            function: fn(&mut A, &mut <A as Actor>::Ctx, M1) -> ReqFlow<A, R>,
            params: &M1,
        ) -> Req<'a, A, M1, R> {
            todo!()
        }
    }

    impl<'a, A: Actor, R, M1> CallRemote<'a, A, M1, &M1, BoxFut<ReqFlow<A, R>>, Req<'a, A, M1, R>>
        for Address<A>
    where
        R: 'static,
        M1: 'static,
    {
        fn call_rem(
            &self,
            function: fn(&mut A, &mut <A as Actor>::Ctx, M1) -> BoxFut<ReqFlow<A, R>>,
            params: &M1,
        ) -> Req<'a, A, M1, R> {
            todo!()
        }
    }

    impl<'a, A: Actor, R, M1, M2>
        CallRemote<'a, A, (M1, M2), (&M1, &M2), ReqFlow<A, R>, Req<'a, A, (M1, M2), R>>
        for Address<A>
    where
        R: 'static,
        M1: 'static,
        M2: 'static,
    {
        fn call_rem(
            &self,
            function: fn(&mut A, &mut <A as Actor>::Ctx, (M1, M2)) -> ReqFlow<A, R>,
            params: (&M1, &M2),
        ) -> Req<'a, A, (M1, M2), R> {
            todo!()
        }
    }

    impl<'a, A: Actor, R, M1, M2>
        CallRemote<'a, A, (M1, M2), (&M1, &M2), BoxFut<ReqFlow<A, R>>, Req<'a, A, (M1, M2), R>>
        for Address<A>
    where
        R: 'static,
        M1: 'static,
        M2: 'static,
    {
        fn call_rem(
            &self,
            function: fn(&mut A, &mut <A as Actor>::Ctx, (M1, M2)) -> BoxFut<ReqFlow<A, R>>,
            params: (&M1, &M2),
        ) -> Req<'a, A, (M1, M2), R> {
            todo!()
        }
    }
}

#[derive(Debug)]
struct MyActor;

#[async_trait::async_trait]
impl Actor for MyActor {
    type Init = Self;
    type ExitError = zestors::AnyhowError;
    type ExitNormal = ();
    type ExitWith = actor::ExitReason<Self>;
    type Ctx = BasicCtx<Self>;
    const ABORT_TIMER: Duration = Duration::from_secs(5);

    async fn init(init: Self::Init, address: Address<Self>) -> InitFlow<Self> {
        InitFlow::Init(init)
    }

    async fn exit(self, reason: actor::ExitReason<Self>) -> Self::ExitWith {
        reason
    }

    fn ctx(&mut self) -> &mut Self::Ctx {
        todo!()
    }
}

impl MyActor {
    pub async fn my_message(
        &mut self,
        _: &mut BasicCtx<Self>,
        params: (u32, u32),
    ) -> MsgFlow<Self> {
        MsgFlow::Ok
    }

    pub async fn say_hello(&mut self, _: &mut BasicCtx<Self>, params: u32) -> MsgFlow<Self> {
        MsgFlow::Ok
    }
}

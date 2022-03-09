use std::pin::Pin;
use std::time::Duration;

use futures::Future;
use uuid::Uuid;
use zestors::actor::{self};
use zestors::address::{Address};
use zestors::context::{BasicCtx, NoCtx};
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
    let token1 = Uuid::new_v4();
    let token2 = Uuid::new_v4();

    let local_node_1 = LocalNode::initialize(token1, 1, "127.0.0.1:1234".parse().unwrap())
        .await
        .unwrap();

    let local_node_2 = LocalNode::initialize(token1, 2, "127.0.0.1:1235".parse().unwrap())
        .await
        .unwrap();

    let local_node_3 = LocalNode::initialize(token1, 3, "127.0.0.1:1236".parse().unwrap())
        .await
        .unwrap();

    local_node_1
        .connect("127.0.0.1:1235".parse().unwrap())
        .await
        .unwrap();

    local_node_2
        .connect("127.0.0.1:1236".parse().unwrap())
        .await
        .unwrap();

    local_node_3
        .connect("127.0.0.1:1234".parse().unwrap())
        .await
        .unwrap();

    main1(local_node_1).await;
    main2(local_node_2).await;
    main3(local_node_3).await;
}

async fn main1(local_node: LocalNode) {
    let node2 = local_node.cluster().get_node(2).unwrap();
    let node3 = local_node.cluster().get_node(3).unwrap();

    let (child, address) = actor::spawn::<MyActor>(MyActor);
    let pid = local_node
        .register(&address, Some("hi".to_string()))
        .unwrap();

    println!("{:?}\n\n{:?}\n\n{:?}", pid.id(), child, address)
}

async fn main2(local_node: LocalNode) {
    let node1 = local_node.cluster().get_node(1).unwrap();
    let node3 = local_node.cluster().get_node(3).unwrap();
}

async fn main3(local_node: LocalNode) {
    let node1 = local_node.cluster().get_node(1).unwrap();
    let node2 = local_node.cluster().get_node(2).unwrap();
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

impl Actor for MyActor {
    type Init = Self;
    type ExitError = zestors::AnyhowError;
    type ExitNormal = ();
    type ExitWith = actor::ExitReason<Self>;
    type Ctx = BasicCtx<Self>;
    const ABORT_TIMER: Duration = Duration::from_secs(5);

    fn init(
        init: Self::Init,
        address: Address<Self>,
    ) -> Pin<Box<dyn Future<Output = InitFlow<Self>> + Send>> {
        Box::pin(async { InitFlow::Init(init) })
    }

    fn exit(self, reason: actor::ExitReason<Self>) -> Self::ExitWith {
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

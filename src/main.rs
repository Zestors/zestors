use std::any::Any;
use std::convert::Infallible;
use std::marker::PhantomData;
use std::pin::Pin;
use std::time::{Duration, Instant};

use async_channel::Sender;
use futures::Future;
use uuid::Uuid;
use zestors::actor::{self, Spawn};
use zestors::address::{Address, Call};
use zestors::child::{Child, ProcessExit};
use zestors::flows::{InitFlow, ReqFlow};
use zestors::inbox::IsUnbounded;
use zestors::messaging::Req;
use zestors::state::State;
use zestors::{
    actor::Actor,
    address::{self, Addressable},
    distributed::{
        local_node::{self, LocalNode},
        node,
    },
    flows::MsgFlow,
    messaging::Msg,
};

#[tokio::main]
pub async fn main() {
    let token1 = Uuid::new_v4();
    let token2 = Uuid::new_v4();

    let time = Instant::now();

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

    println!("{:?}", local_node_1.get_node(2));
    println!("{:?}", local_node_1.get_node(3));
    println!("{:?}", local_node_2.get_node(1));
    println!("{:?}", local_node_2.get_node(3)); 
    println!("{:?}", local_node_3.get_node(1));
    println!("{:?}", local_node_3.get_node(2));
}

#[derive(Debug)]
struct C2(i64);

impl Actor for C2 {
    type Init = i64;
    type ExitError = zestors::AnyhowError;
    type ExitNormal = ();
    type ExitWith = Self;
    type State = zestors::state::State<Self>;
    type Inbox = zestors::inbox::Unbounded;
    const INBOX_CAPACITY: usize = 0;
    const ABORT_TIMER: std::time::Duration = std::time::Duration::from_secs(5);

    fn init(init: Self::Init, state: &mut Self::State) -> zestors::flows::InitFlow<Self> {
        InitFlow::Init(Self(init))
    }

    fn handle_exit(self, state: Self::State, reason: zestors::actor::ExitReason<Self>) -> Self {
        self
    }
}

#[derive(Debug)]
struct Calculator(i64);

impl Actor for Calculator {
    type Init = i64;
    type ExitError = zestors::AnyhowError;
    type ExitNormal = ();
    type ExitWith = Self;
    type State = zestors::state::State<Self>;
    type Inbox = zestors::inbox::Unbounded;
    const INBOX_CAPACITY: usize = 0;
    const ABORT_TIMER: std::time::Duration = std::time::Duration::from_secs(5);

    fn init(init: Self::Init, state: &mut Self::State) -> zestors::flows::InitFlow<Self> {
        InitFlow::Init(Self(init))
    }

    fn handle_exit(self, state: Self::State, reason: zestors::actor::ExitReason<Self>) -> Self {
        self
    }
}

pub trait Supervises<A: Actor> {
    fn supervise(&self, process: Child<A>) -> Msg<A, Child<A>>;
    fn spawn_and_supervise(&self, init: A::Init) -> Req<A, <A as Actor>::Init, Address<A>>;
}

impl Supervises<Calculator> for Address<Calculator> {
    fn supervise(&self, address: Child<Calculator>) -> Msg<'_, Calculator, Child<Calculator>> {
        self.msg(
            |_, state, address| {
                state.schedule_and_then(address, Calculator::supervise_calculator);

                MsgFlow::Ok
            },
            address,
        )
    }

    fn spawn_and_supervise(
        &self,
        init: <Calculator as Actor>::Init,
    ) -> Req<Calculator, <Calculator as Actor>::Init, Address<Calculator>> {
        self.req(
            |_, state, init| {
                let (pid, address) = actor::spawn::<Calculator>(init);

                state.schedule_and_then(pid, Calculator::supervise_calculator);

                ReqFlow::Reply(address)
            },
            init,
        )
    }
}

impl Supervises<C2> for Address<Calculator> {
    fn supervise(&self, address: Child<C2>) -> Msg<'_, C2, Child<C2>> {
        todo!()
    }

    fn spawn_and_supervise(
        &self,
        init: <C2 as Actor>::Init,
    ) -> Req<C2, <C2 as Actor>::Init, Address<C2>> {
        todo!()
    }
}

pub trait SomeTrait: Actor {
    fn something(&mut self, state: &mut Self::State, p: ()) -> MsgFlow<Self>
    where
        Self: Sized;
}

impl SomeTrait for Calculator {
    fn something(&mut self, state: &mut Self::State, p: ()) -> MsgFlow<Self> {
        MsgFlow::Ok
    }
}

impl Calculator {
    fn spawn_sub(
        &mut self,
        state: &mut State<Self>,
        init: i64,
    ) -> ReqFlow<Self, Address<Calculator>> {
        let (pid, address) = actor::spawn::<Calculator>(init);

        state.schedule_and_then(pid, Calculator::supervise_calculator);

        ReqFlow::Reply(address)
    }

    fn msg2(&mut self, _: &mut State<Self>, params: (u32, u32)) -> MsgFlow<Self> {
        MsgFlow::Ok
    }

    async fn msg2_async(&mut self, _: &mut State<Self>, params: (u32, u32)) -> MsgFlow<Self> {
        // Box::pin(async move {
        println!("Hi!");
        MsgFlow::Ok
        // })
    }

    fn supervise(
        &mut self,
        state: &mut State<Self>,
        process: Child<Calculator>,
    ) -> ReqFlow<Self, ()> {
        state.schedule_and_then(process, Calculator::supervise_calculator);

        ReqFlow::Reply(())
    }

    fn supervise_calculator(
        &mut self,
        state: &mut State<Self>,
        exit: ProcessExit<Calculator>,
    ) -> MsgFlow<Self> {
        match exit {
            ProcessExit::Handled(calculator) => {
                // Just restart the calculator
                let (pid, _) = actor::spawn::<Calculator>(calculator.0);
                state.schedule_and_then(pid, Calculator::supervise_calculator)
            }
            ProcessExit::InitFailed | ProcessExit::Panic(_) | ProcessExit::HardAbort => {
                // Create a new calculator and spawn it
                let (pid, _) = actor::spawn::<Calculator>(0);
                state.schedule_and_then(pid, Calculator::supervise_calculator)
            }
        }
        MsgFlow::Ok
    }
}

pub trait HandlesTrait {
    fn test() -> Box<Self>
    where
        Self: Sized;
}

pub struct Handles<M: ?Sized> {
    m: PhantomData<M>,
}

struct DynAddressInner;

type BoxFut<T> = Pin<Box<dyn Future<Output = T>>>;

struct DynAddress<T: ?Sized> {
    t: PhantomData<T>,
    handle_pointer: usize,
    address: DynAddressInner,
}

struct DynSomeTrait;

// fn testing(
//     boxed: DynAddress<DynSomeTrait>,
//     boxed2: Box<
//         dyn Actor<
//             Init = (),
//             ExitError = (),
//             ExitNormal = (),
//             ExitWith = (),
//             State = (),
//             Inbox = (),
//         >,
//     >,
// ) {
// }

trait CallRemote<'a, A: Actor, M, P, F, R: 'a> {
    fn call_rem(&'a self, function: fn(&mut A, &mut A::State, M) -> F, params: P) -> R;
}

mod call_pid {
    use super::*;

    impl<'a, A: Actor, M1> CallRemote<'a, A, M1, &M1, MsgFlow<A>, Msg<'a, A, M1>> for Address<A>
    where
        M1: 'static,
    {
        fn call_rem(
            &self,
            function: fn(&mut A, &mut <A as Actor>::State, M1) -> MsgFlow<A>,
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
            function: fn(&mut A, &mut <A as Actor>::State, M1) -> BoxFut<MsgFlow<A>>,
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
            function: fn(&mut A, &mut <A as Actor>::State, (M1, M2)) -> MsgFlow<A>,
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
            function: fn(&mut A, &mut <A as Actor>::State, (M1, M2)) -> BoxFut<MsgFlow<A>>,
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
            function: fn(&mut A, &mut <A as Actor>::State, M1) -> ReqFlow<A, R>,
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
            function: fn(&mut A, &mut <A as Actor>::State, M1) -> BoxFut<ReqFlow<A, R>>,
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
            function: fn(&mut A, &mut <A as Actor>::State, (M1, M2)) -> ReqFlow<A, R>,
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
            function: fn(&mut A, &mut <A as Actor>::State, (M1, M2)) -> BoxFut<ReqFlow<A, R>>,
            params: (&M1, &M2),
        ) -> Req<'a, A, (M1, M2), R> {
            todo!()
        }
    }
}

async fn main2() {
    let (child, address1) = actor::spawn::<Calculator>(0);
    let (child2, address2) = actor::spawn::<Calculator>(10);

    let address = address1
        .call(Calculator::spawn_sub, 10)
        .send_recv()
        .await
        .unwrap();

    let reply = address1.call(Calculator::supervise, child).send().unwrap();

    // let exit = pid2.await;

    // let res = address1.msg_ref(Calculator::supervise_calculator, &exit);

    let res = address1.call_rem(Calculator::msg2, (&10, &10));
    let res = address1.call_rem(Calculator::msg2, &(10, 10));
    // let res = address1.call_rem(Calculator::msg2_async, (&10, &10));
    // let res = address1.call_rem(Calculator::msg2_async, (&10, &10));
    let res = address1.call_rem(Calculator::spawn_sub, &10);

    let res = address1.call(Calculator::msg2, (10, 10));
    let res = address1.call(Calculator::msg2_async, (10, 10));
    let res = address1.call(Calculator::spawn_sub, 10);

    let res = child2.call(Calculator::msg2, (10, 10));
    let res = child2.call(Calculator::msg2_async, (10, 10));
    let res = child2.call(Calculator::spawn_sub, 10);

    // let res = <Address<Calculator> as Supervises<Calculator>>::spawn_and_supervise(&address1, 10)
    //     .send_recv()
    //     .await;

    // let addr = match res {
    //     Ok(addr) => addr,
    //     Err(e) => match e {
    //         SendRecvError::ActorDied(msg) => todo!(),
    //         SendRecvError::ActorDiedAfterSending => todo!(),
    //     },
    // };
}

pub trait Message {
    type Params;
    type Flow;
}

struct DynamicAddress1<M: Message> {
    sender: Sender<Box<dyn Any>>,
    p: PhantomData<M>,
    ptr: usize,
}

impl<M: Message> DynamicAddress1<M> {
    pub fn new<A: Actor>(address: Address<A>) -> Self
    where
        A: SomeTrait,
    {
        let (sender, _) = async_channel::bounded(10);
        Self {
            sender,
            p: PhantomData,
            ptr: <A as SomeTrait>::something as usize,
        }
    }

    pub fn send(&self, params: M::Params) {}
}

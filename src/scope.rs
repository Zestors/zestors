use std::{marker::PhantomData, time::Duration};

use futures::future;

use crate::{
    actor::{self, Actor, ExitReason},
    address::{Address, Addressable},
    context::NoCtx,
    errors::{DidntArrive, NoReply},
    flows::{InitFlow, ReqFlow},
    function::{ReqFn},
    messaging::Reply, Fn,
};

struct Scope<'scope, R> {
    vec: Vec<Reply<R>>,
    scope: PhantomData<&'scope ()>,
}

impl<'scope, R> Scope<'scope, R>
where
    R: 'scope + Send,
{
    fn send<A: Actor, P>(
        &mut self,
        address: &Address<A>,
        req_fn: ReqFn<A, fn(P) -> R>,
        params: P,
    ) -> Result<(), DidntArrive<P>>
    where
        P: 'scope + Send,
    {
        match unsafe { address.req_ref(req_fn, params) } {
            Ok(reply) => {
                self.vec.push(reply);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    async unsafe fn new<T>(function: impl FnOnce(&mut Self) -> T) -> (Vec<Result<R, NoReply>>, T)
    where
        T: 'static,
        R: 'scope,
    {
        let mut scope = Scope {
            vec: Vec::new(),
            scope: PhantomData,
        };

        let t = function(&mut scope);

        let vec = future::join_all(scope.vec).await;

        (vec, t)
    }

    // fn send_map<A, P, R2>(
    //     &mut self,
    //     address: &Address<A>,
    //     req_fn: ReqFn<A, P, R2, RefSafe>,
    //     params: P,
    //     map: impl FnOnce(R2) -> R,
    // ) -> Result<(), DidntArrive<P>>
    // where
    //     P: 'scope,
    //     R2: 'scope,
    // {
    //     todo!()
    // }
}

#[macro_export]
macro_rules! scope {
    ($function:expr) => {
        unsafe { Scope::new($function).await }
    };
}

struct Fn1;
struct Fn2;

trait Test<P, R, Fn>: Sized {
    fn test(self, p: P) -> R;
}

impl<P, R, T> Test<P, R, Fn1> for T
where
    T: Fn(P) -> R,
{
    fn test(self, p: P) -> R {
        self(p)
    }
}


impl<P, R, T> Test<P, R, Fn2> for T
where
    T: Fn(P, P) -> R,
    P: Clone
{
    fn test(self, p: P) -> R {
        self(p.clone(), p)
    }
}

fn test_fn(str: &str) -> &str {
    todo!()
}

#[derive(Debug)]
pub struct Struct<T>(T);

#[tokio::test]
async fn test2() {
    let child = actor::spawn(MyActor);
    let address = child.addr();

    let val = "hi".to_string();
    let val2 = test_fn.test(&val);
    // drop(val);
    println!("{:?}", val2);

    let val = "hi".to_string();
    let val2 = address
        .req_recv(Fn!(MyActor::say_hello3), &val)
        .await
        .unwrap();

    // drop(val);
    println!("{:?}", val2);

    let str = "hi".to_string();
    let u32 = 10;

    let (results, operation) = scope!(|scope| {
        scope.send(&address, Fn!(MyActor::say_hello3), &str).unwrap();
        scope.send(&address, Fn!(MyActor::say_hello3), &str).unwrap();
        scope.send(&address, Fn!(MyActor::say_hello3), &str).unwrap();

        expensive_operation_while_waiting()
    });

    // drop(str);
    drop(u32);

    println!("res: {:?}", results);
    // println!("res: {:?}", reply);
    println!("operation: {:?}", operation);
}

fn expensive_operation_while_waiting() -> u32 {
    10
}

#[derive(Debug)]
struct MyActor;

#[async_trait::async_trait]
impl Actor for MyActor {
    type Init = Self;
    type ExitError = crate::AnyhowError;
    type ExitNormal = ();
    type ExitWith = ExitReason<Self>;
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
    pub async fn say_hello2(&mut self, _: &str) -> ReqFlow<Self, u32> {
        ReqFlow::Reply(10)
    }

    pub fn say_hello3<'a>(&mut self, val: &'a str) -> ReqFlow<Self, &'a str> {
        ReqFlow::Reply(val)
    }
}

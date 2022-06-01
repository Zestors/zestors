use std::{error::Error, fmt::format, marker::PhantomData, task::Poll};
use zestors::{core::*, distr::distr_addr::Distr, Action, Fn};
use zestors_codegen::{zestors, Addr, NoScheduler, Scheduler};

impl<T> Unpin for MyActorScheduler<T> {}
#[derive(Debug)]
struct MyActorScheduler<T>(PhantomData<T>);

impl<T: Send + 'static + Clone> zestors::core::Stream for MyActorScheduler<T> {
    type Item = Action<MyActor<T>>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }
}

//------------------------------------------------------------------------------------------------
//  MyActor2
//------------------------------------------------------------------------------------------------

#[derive(Addr, NoScheduler)]
struct MyActor2<T: Send + 'static>
where
    T: Clone,
{
    val: T,
}

#[async_trait]
impl<T: Clone + Send + 'static> Actor for MyActor2<T> {
    type Init = T;

    type Error = Box<dyn Error + Send>;
    type Halt = ();

    type Exit = zestors::core::Signal<Self>;

    async fn initialize(init: Self::Init, addr: Self::Addr) -> InitFlow<Self> {
        InitFlow::Init(Self { val: init })
    }

    async fn handle_signal(self, event: Signal<Self>, state: &mut State<Self>) -> SignalFlow<Self> {
        SignalFlow::Exit(event)
    }
}

//------------------------------------------------------------------------------------------------
//  MyActor
//------------------------------------------------------------------------------------------------

#[derive(Addr, Scheduler, Debug)]
struct MyActor<T: Send + 'static + Clone> {
    val: T,
    #[scheduler]
    scheduler: MyActorScheduler<T>,
}

#[async_trait]
impl<T: Clone + Send + 'static> Actor for MyActor<T> {
    type Init = T;
    type Error = Box<dyn Error + Send>;
    type Exit = Signal<Self>;
    type Halt = ();

    async fn initialize(init: Self::Init, addr: MyActorAddr<T>) -> InitFlow<Self> {
        InitFlow::Init(Self {
            val: init,
            scheduler: MyActorScheduler(PhantomData),
        })
    }

    async fn handle_signal(self, signal: Signal<Self>, state: &mut State<Self>) -> SignalFlow<Self> {
        SignalFlow::Exit(signal)
    }
}

#[zestors]
impl<T: Clone + Send> MyActor<T>
where
    T: 'static,
{
    /// This function handles a message P, and parameter of string, and sends back a string based on
    /// the type_ids of these two values.
    #[As(pub hi44)]
    pub(crate) fn handle_hi_test<P: Send>(
        &mut self,
        (msg1, msg2): (P, u32),
        snd: Snd<String>,
    ) -> FlowResult<Self>
    where
        P: 'static,
    {
        Ok(Flow::Cont)
    }

    pub(crate) fn handle_hi_test3(
        &mut self,
        (msg1, msg2): (String, u32),
        snd: Snd<String>,
    ) -> FlowResult<Self> {
        Ok(Flow::Cont)
    }
}

impl<T: Clone + Send + 'static> MyActor<T> {
    #[doc = "test1"]
    #[doc = "test1"]
    /// #[As(hi)]
    pub(crate) fn handle_hi(&mut self, message: String) -> FlowResult<Self> {
        let test = <String>::clone;

        // <MyActor>::handle_hi();

        Ok(Flow::Cont)
    }

    pub(crate) fn handle_rcv(&mut self, message: (Rcv<u32>, String)) -> FlowResult<Self> {
        Ok(Flow::Cont)
    }

    // #[As(hi_double)]
    pub(crate) fn handle_hi_double(
        &mut self,
        (message1, message2): (String, String),
    ) -> FlowResult<Self> {
        Ok(Flow::Cont)
    }

    // #[As(echo)]
    pub(crate) fn handle_echo(&mut self, message: String, snd: Snd<String>) -> FlowResult<Self> {
        snd.send(message).unwrap();
        Ok(Flow::Cont)
    }

    // #[As(echo_double)]
    pub(crate) fn handle_echo_double(
        &mut self,
        (message1, message2): (String, String),
        snd: Snd<String>,
    ) -> FlowResult<Self> {
        snd.send(format!("{} - {}", message1, message2)).unwrap();
        Ok(Flow::Cont)
    }
}

pub trait EchoAddr<AT = Local>
where
    Self: Address<AddrType = AT>,
    AT: AddrType,
{
    fn trait_echo<'z>(
        &self,
        message: <String as ParamType<'z, AT>>::Param,
    ) -> <AT as AddrType>::CallResult<String, Rcv<String>>
    where
        String: ParamType<'z, AT>,
        <String as ParamType<'z, AT>>::Param: Into<<AT as AddrType>::Msg<String>>;
}

impl<T, AT> EchoAddr<AT> for MyActorAddr<T, AT>
where
    T: Send + 'static + Clone,
    AT: AddrType,
    Self: Address<AddrType = AT>,
{
    fn trait_echo<'z>(
        &self,
        message: ParamT<'z, String, AT>,
    ) -> <AT as AddrType>::CallResult<String, Rcv<String>>
    where
        String: ParamType<'z, AT>,
        ParamT<'z, String, AT>: Into<<AT as AddrType>::Msg<String>>,
    {
        self.call(Fn!(MyActor::<T>::handle_echo), message)
    }
}

#[allow(unused)]
#[tokio::test]
async fn test_basic_actor_works() {
    let (mut child, addr): (_, MyActorAddr<u32>) = spawn_actor::<MyActor<u32>>(10);

    let res = addr.call(
        Fn!(MyActor::<u32>::handle_echo_double),
        ("hi".to_string(), "hello".to_string()),
    );

    let res = addr.man_hi("helli".to_string());
    let res = addr.man_hi2("helli".to_string(), "helli2".to_string());

    assert_eq!(
        addr.man_echo("hi".to_string()).unwrap().await.unwrap(),
        "hi".to_string()
    );

    assert_eq!(
        addr.man_echo2("hi".to_string(), "hello".to_string())
            .unwrap()
            .await
            .unwrap(),
        "hi - hello".to_string()
    );

    // let _ = addr.hi_test::<u32>(10, 20);

    let addr: Box<dyn EchoAddr> = Box::new(addr);
    let addr2 = addr.clone();

    assert_eq!(
        addr.trait_echo("hi".to_string()).unwrap().await.unwrap(),
        "hi".to_string()
    );

    assert_eq!(
        addr2.trait_echo("hi".to_string()).unwrap().await.unwrap(),
        "hi".to_string()
    );

    let _addr: MyActorAddr<u32> = addr.downcast().unwrap();
    let _addr: MyActorAddr<u32> = addr2.downcast().unwrap();

    child.soft_abort();
    match child.await {
        Ok(res) => match res {
            Signal::Actor(ActorSignal::SoftAbort) => (),
            _ => panic!(),
        },
        Err(_) => panic!(),
    }
}

//------------------------------------------------------------------------------------------------
//  Manual implementation
//------------------------------------------------------------------------------------------------

impl<'z, T: Clone + Send + 'static, AT: AddrType> MyActorAddr<T, AT> {
    pub(crate) fn man_hi<P1>(&self, message: P1) -> AT::CallResult<String, ()>
    where
        P1: Into<AT::Msg<String>>,
    {
        self.call(Fn!(MyActor::<T>::handle_hi), message)
    }

    pub(crate) fn man_hi2<P1, P2>(
        &self,
        message1: P1,
        message2: P2,
    ) -> CallResultT<AT, (String, String), ()>
    where
        (P1, P2): Into<AT::Msg<(String, String)>>,
    {
        self.call(Fn!(MyActor::<T>::handle_hi_double), (message1, message2))
    }

    pub(crate) fn man_rcv_v1(
        &self,
        rcv: ParamT<'z, Rcv<u32>, AT>,
        msg: ParamT<'z, String, AT>,
    ) -> CallResultT<AT, (Rcv<u32>, String), ()>
    where
        String: ParamType<'z, AT>,
        Rcv<u32>: ParamType<'z, AT>,
        (ParamT<'z, Rcv<u32>, AT>, ParamT<'z, String, AT>): Into<AT::Msg<(Rcv<u32>, String)>>,
    {
        self.call(Fn!(MyActor::<T>::handle_rcv), (rcv, msg))
    }

    pub(crate) fn man_rcv_v2<P1, P2>(
        &self,
        message1: P1,
        message2: P2,
    ) -> AT::CallResult<(Rcv<u32>, String), ()>
    where
        (P1, P2): Into<AT::Msg<(Rcv<u32>, String)>>,
    {
        self.call(Fn!(MyActor::<T>::handle_rcv), (message1, message2))
    }

    pub(crate) fn man_echo(
        &self,
        message: ParamT<'z, String, AT>,
    ) -> CallResultT<AT, String, Rcv<String>>
    where
        String: ParamType<'z, AT>,
        ParamT<'z, String, AT>: Into<AT::Msg<String>>,
    {
        self.call(Fn!(MyActor::<T>::handle_echo), message)
    }

    pub(crate) fn man_echo2(
        &self,
        message1: ParamT<'z, String, AT>,
        message2: ParamT<'z, String, AT>,
    ) -> CallResultT<AT, (String, String), Rcv<String>>
    where
        String: ParamType<'z, AT>,
        (ParamT<'z, String, AT>, ParamT<'z, String, AT>): Into<AT::Msg<(String, String)>>,
    {
        self.call(Fn!(MyActor::<T>::handle_echo_double), (message1, message2))
    }
}

fn test(addr: MyActorAddr<u32>, remote_addr: MyActorAddr<u32, Distr>) {
    let res: Result<Rcv<String>, LocalAddrError<String>> = addr.man_echo("hi".to_string());
    let res: Result<Rcv<String>, LocalAddrError<String>> = remote_addr.man_echo(&"hi".to_string());

    let _ = remote_addr.man_echo2(&"hello".to_string(), &"hello".to_string());
    let _ = remote_addr.man_hi2(&"hello".to_string(), &"hello".to_string());

    let (snd, rcv) = new_channel();
    let _ = addr.man_rcv_v1(rcv, "hello".to_string());
    let (snd, rcv) = new_channel();
    let _ = remote_addr.man_rcv_v1(rcv, &"hello".to_string());

    let res = addr.call(
        Fn!(MyActor::<u32>::handle_echo_double),
        ("hi".to_string(), "hello".to_string()),
    );

    let res = remote_addr.call(
        Fn!(MyActor::<u32>::handle_echo_double),
        (&"hi".to_string(), &"hello".to_string()),
    );

    let (snd, rcv) = new_channel();
    let res = addr.call(Fn!(MyActor::<u32>::handle_rcv), (rcv, "hi".to_string()));

    let (snd, rcv) = new_channel();
    let res = remote_addr.call(Fn!(MyActor::<u32>::handle_rcv), (rcv, &"hi".to_string()));

    let addr2: Box<dyn EchoAddr> = Box::new(addr);
    let remote_addr2: Box<dyn EchoAddr<Distr>> = Box::new(remote_addr.clone());

    let _ = remote_addr2.trait_echo(&"hi".to_string());
    let _ = addr2.trait_echo("hi".to_string());

    let res: Result<Rcv<String>, LocalAddrError<(String, String)>> = remote_addr.call(
        Fn!(MyActor::<u32>::handle_echo_double),
        (&"hi".to_string(), &"hello".to_string()),
    );
}

#[test]
fn test_action_downcasting() {
    let (action, _rcv) = Action::new_split(Fn!(MyActor::<u32>::handle_echo), "hi".to_string());
    action.downcast::<String, Rcv<String>>().unwrap();
    let (action, ()) = Action::new_split(Fn!(MyActor::<u32>::handle_hi), "hi".to_string());
    action.downcast::<String, ()>().unwrap();
}

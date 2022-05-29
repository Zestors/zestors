//------------------------------------------------------------------------------------------------
//  struct MyActor
//------------------------------------------------------------------------------------------------

// #[Scheduler(None)]

#[derive(Addr, Scheduler)]
#[Addr(MyActorAddr)]
#[RemoteAddr(MyActorRemoteAddr)]
#[Scheduler(long.path.scheduler())]
struct MyActor {
    scheduler: GenericScheduler<Self>,
    address: MyActorAddr,
}

//-------------------------------------------------
//  Generated
//-------------------------------------------------

struct MyActorAddr(Addr<MyActor>);
struct MyActorRemoteAddr(RemoteAddr<MyActor>);

impl Stream for MyActor {
    type Item = Action<Self>;

    fn poll_next() -> Poll {
        self.long.path.scheduler().poll_next_unpin()
    }
}

trait IsLocalAddr: IsAddr<AddrType = Local> {}
trait IsRemoteAddr: IsAddr<AddrType = Remote> {}

impl IsAddr for MyActorAddr {
    type AddrType = Local;

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_mut_any(&mut self) -> &mut dyn Any {
        self
    }
}

impl IsAddr for MyRemoteActorAddr {
    type AddrType = Remote;

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_mut_any(&mut self) -> &mut dyn Any {
        self
    }
}

impl LocalAddr for MyActorAddr {
    type Actor = MyActor;

    fn from_addr(addr: Addr<MyActor>) -> Self {
        Self(addr)
    }
    fn as_addr(&self) -> &Addr<MyActor> {
        &self.0
    }
}

impl RemoteAddr for MyActorRemoteAddr {
    type Actor = MyActor;

    fn from_addr(addr: RemoteAddr<MyActor>) -> Self {
        Self(addr)
    }
    fn as_addr(&self) -> &RemoteAddr<MyActor> {
        &self.0
    }
}

//------------------------------------------------------------------------------------------------
//  impl Actor
//------------------------------------------------------------------------------------------------

impl Actor for MyActor {
    type Init = ();
    type Error = anyhow::Error;
    type Exit = Self;
    type Addr = MyActorAddr;

    fn init(init: Self::Init, addr: Self::Addr) -> InitFlow<Self> {
        InitFlow::Init(Self {
            scheduler: GenericScheduler::new(),
            addr,
        })
    }

    fn handle_signal(self, signal: Signal<Self>, state: State<Self>) -> SignalFlow<Self> {
        SignalFlow::Exit(self)
    }
}

impl RemoteActor for MyActor {
    type RemoteAddr = MyActorRemoteAddr;
}

//------------------------------------------------------------------------------------------------
//  Impl
//------------------------------------------------------------------------------------------------

#[zestors]
#[Impl(MyActorAddr, MyActorRemoteAddr)]
impl MyActor {
    #[As(send_hi)]
    pub(crate) fn handle_hi(&mut self, params: String) -> Result<Flow, Error> {
        Ok(Flow::Cont)
    }

    #[Inline]
    fn send_any<T>(&mut self, params: T) -> Result<Flow, Error> {
        Ok(Flow::Cont)
    }

    #[For(MyActorAddr)]
    #[As(echo_str)]
    #[Vis(pub)]
    fn handle_echo_str(&mut self, params: String, request: Request<String>) -> Result<Flow, Error> {
        Self::handle_echo(&mut self, params, request)
    }

    fn some_other_fn(&self, val: u32) -> u32 {
        val
    }
}

//-------------------------------------------------
//  Generated
//-------------------------------------------------

impl MyActor {
    pub(crate) fn handle_hi(&mut self, params: String) -> Result<Flow, Error> {
        Ok(Flow::Cont)
    }

    fn handle_echo_str(&mut self, params: String, request: Request<String>) -> Result<Flow, Error> {
        Self::handle_echo(&mut self, params, request)
    }

    fn some_other_fn(&self, val: u32) -> u32 {
        val
    }
}

impl MyActorAddr {
    pub(crate) fn send_hi(&self, params: String) -> Result<(), SendError> {
        self.call(Fn!(MyActor::handle_hi), params)
    }

    fn send_any<T>(&self, params: T) -> Result<Flow, Error> {
        fn __inlined_handle_any__<T>(__actor__: &mut MyActor, params: T) -> Result<Flow, Error> {
            Ok(Flow::Cont)
        }

        self.call(Fn!(__inlined_handle_any__::<T>), params)
    }

    pub fn echo_str(&self, params: String) -> Result<Reply<String>, SendError> {
        self.call(Fn!(MyActor::handle_echo), params)
    }
}

impl MyActorRemoteAddr {
    pub(crate) fn send_hi(&self, params: String) -> Result<(), RemoteSendError> {
        self.call(Fn!(MyActor::handle_hi), params)
    }

    fn send_any<T>(&self, params: T) -> Result<Flow, RemoteSendError> {
        fn __inlined_handle_any__<T>(__actor__: &mut MyActor, params: T) -> Result<Flow, Error> {
            Ok(Flow::Cont)
        }

        self.call(Fn!(__inlined_handle_any__::<T>), params)
    }
}

//------------------------------------------------------------------------------------------------
//  new Trait
//------------------------------------------------------------------------------------------------

pub trait EchoAddr<T>: IsAddr {
    type SomeType;

    fn echo(&self, params: T) -> Result<Reply<T>, <Self::AddrType as AddrType>::CallError>;
    fn echo2(&self, params: T) -> Result<Reply<T>, <Self::AddrType as AddrType>::CallError>;
    fn do_nothing(&self);
}

//------------------------------------------------------------------------------------------------
//  Impl Trait
//------------------------------------------------------------------------------------------------

#[zestors]
#[Impl(EchoAddr::<T>)]
#[For(MyActorAddr, MyActorRemoteAddr)]
#[Where(T: Debug)]
impl MyActor {
    #[Raw]
    type SomeType = ();

    #[As(echo)]
    fn handle_echo<T>(&mut self, params: T, request: Request<T>) -> Result<Flow, Error>
    where
        T: Debug,
    {
        println!("{:?}", params);
        request.reply(params);
        Ok(Flow::Cont)
    }

    #[Inline]
    fn echo2<T>(&mut self, params: T, request: Request<T>) -> Result<Flow, Error> {
        request.reply(params);
        Ok(Flow::Cont)
    }

    #[Raw]
    fn do_nothing(&self) {}
}

//-------------------------------------------------
//  Generated
//-------------------------------------------------

impl MyActor {
    fn handle_echo<T>(&mut self, params: T, request: Request<T>) -> Result<Flow, Error>
    where
        T: Debug,
    {
        println!("{:?}", params);
        request.reply(params);
        Ok(Flow::Cont)
    }
}

impl EchoAddr<T> for MyActorAddr
where
    T: Debug,
{
    type SomeType = ();

    fn echo(&self, params: T) -> Result<Reply<T>, SendError> {
        self.call(Fn!(MyActor::handle_echo::<T>), params)
    }

    fn echo2(&self, params: T) -> Result<Reply<T>, SendError> {
        fn __inlined_echo2__<T>(
            __actor__: &mut MyActor,
            params: T,
            request: Request<T>,
        ) -> Result<Flow, Error> {
            println!("{:?}", params);
            request.reply(params);
            Ok(Flow::Cont)
        }

        self.call(Fn!(__inlined_echo2__::<T>), params)
    }

    fn do_nothing(&self) {}
}

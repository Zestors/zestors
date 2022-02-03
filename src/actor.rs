use std::{marker::PhantomData, ops::Add};

use crate::{
    messaging::{PacketReceiver, PacketSender},
    packets::Packet, address::Address,
};



//-------------------------------------
// Actor
//-------------------------------------

pub trait Actor: Send + Sync + 'static + Sized {
    /// The error type that can be used to stop this [Actor].
    type Error: Send;

    /// The type that is returned when this [Actor] exits.
    type Exit: Send;

    /// The [State] that is passed to the functions, default is [BasicState].
    /// Needs to implement [State<Self>]
    type State: State<Self>;

    /// Spawn this [Actor], and return an [Address] of this [Actor].
    fn spawn(self) -> Child<Self> {
        // create the packet senders
        let (sender, receiver) = PacketSender::new(None);

        // create the address from the sender
        let address = Address { sender };

        // create the state
        let state = Self::State::starting(&self, address.clone());

        // start the actor
        let child = RunningActor::new(self, state, receiver).start(address);

        // return the address
        child
    }

    fn starting(&mut self, state: &mut Self::State);

    fn stopping(self, state: &mut Self::State, exit: ActorExit<Self::Error>) -> Self::Exit;
}

//-------------------------------------
// RunningActor
//-------------------------------------

struct RunningActor<A: Actor<State = S>, S: State<A>> {
    actor: A,
    state: S,
    receiver: PacketReceiver<A>,
}

impl<A: Actor<State = S> + Send + 'static, S: State<A> + Send + 'static> RunningActor<A, S> {
    pub fn new(actor: A, state: S, receiver: PacketReceiver<A>) -> Self {
        Self {
            actor,
            state,
            receiver,
        }
    }

    pub fn start(self, address: Address<A>) -> Child<A> {
        let handle = tokio::task::spawn(async move {
            Self::started(self.actor, self.state, self.receiver).await
        });

        Child::new(handle, address)
    }

    async fn started(
        mut actor: A,
        mut state: S,
        receiver: PacketReceiver<A>,
    ) -> <A as Actor>::Exit {
        actor.starting(&mut state);

        let actor_exit = loop {
            if let Some(Packet {
                handler_fn,
                handler_packet,
            }) = receiver.recv().await
            {
                let flow = match handler_fn {
                    crate::packets::HandlerFn::Sync(handler_fn) => unsafe {
                        handler_fn(&mut actor, &mut state, handler_packet)
                    },
                    crate::packets::HandlerFn::Async(handler_fn) => unsafe {
                        handler_fn(&mut actor, &mut state, handler_packet).await
                    },
                };

                match flow {
                    crate::flow::MsgFlow::Ok => (),
                    crate::flow::MsgFlow::OkAnd(_) => todo!(),
                    crate::flow::MsgFlow::Stop(e) => break ActorExit::Error(e),
                    crate::flow::MsgFlow::StopNormal => break ActorExit::Normal,
                }
            } else {
                break ActorExit::Normal;
            }
        };

        actor.stopping(&mut state, actor_exit)
    }
}

//-------------------------------------
// Child
//-------------------------------------

pub struct Child<A: Actor> {
    handle: tokio::task::JoinHandle<<A as Actor>::Exit>,
    address: Address<A>,
}

impl<A: Actor> Child<A> {
    pub(crate) fn new(
        handle: tokio::task::JoinHandle<<A as Actor>::Exit>,
        address: Address<A>,
    ) -> Self {
        Self { handle, address }
    }

    pub fn clone_address(&self) -> Address<A> {
        self.address.clone()
    }
}

impl<A: Actor> std::ops::Deref for Child<A> {
    type Target = Address<A>;

    fn deref(&self) -> &Self::Target {
        &self.address
    }
}

//-------------------------------------
// ActorExit
//-------------------------------------

pub enum ActorExit<E> {
    Error(E),
    Normal,
    AddressesDropped,
}

//-------------------------------------
// State
//-------------------------------------

pub trait State<A: Actor + ?Sized>: Send + 'static {
    fn starting(actor: &A, address: Address<A>) -> Self;
    fn stopping(&mut self, actor: &A);
    fn address(&self) -> &Address<A>;
}

pub struct BasicState<A: Actor + ?Sized> {
    address: Address<A>,
}

impl<A: Actor + 'static> State<A> for BasicState<A> {
    fn starting(actor: &A, address: Address<A>) -> Self {
        Self { address }
    }

    fn stopping(&mut self, actor: &A) {}

    fn address(&self) -> &Address<A> {
        &self.address
    }
}

unsafe impl<A: Actor> Send for BasicState<A> {}

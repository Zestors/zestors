use std::pin::Pin;

use futures::{
    future::Either,
    pin_mut,
    stream::{self, select_all, select_with_strategy, Next, PollNext},
    Future, StreamExt,
};
use tokio::pin;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::{
    action::{After, Before},
    address::{Address, FromAddress},
    flow::{Flow, InternalFlow, MsgFlow},
    messaging::{PacketReceiver, PacketSender},
    packets::{HandlerFn, Packet},
    state::{self, ActorState, Scheduled, State},
    AnyhowError,
};

//-------------------------------------
// Actor
//-------------------------------------

pub trait InboxType {}

impl InboxType for Unbounded {}
impl InboxType for Bounded {}

pub(crate) trait IsBounded {}
pub(crate) trait IsUnbounded {}

impl IsUnbounded for Unbounded {}
impl IsBounded for Bounded {}

pub struct Unbounded;
pub struct Bounded;

// pub trait TrySending2<'a, 'b, I, F, P, R, G, A> {
//     fn trysend(&'a self, function: RemoteFunction<F>, params: P) -> Result<R, TrySendError<P>>;
// }

// trait TrySending<A: Actor, C> {
//     // fn try_send(&self, )
// }

// impl<A: Actor, T> TrySending<A, Unbounded> for T where A::Inbox: IsBounded {}

// impl<A: Actor, T> TrySending<A, Bounded> for T where A::Inbox: IsUnbounded {}

pub trait Actor: Send + Sync + 'static + Sized {
    /// The error type that can be used to stop this [Actor].
    type ErrorExit = AnyhowError;
    /// The type that is used to stop this [Actor] normally.
    type NormalExit = ();
    /// The type that is returned when this [Actor] exits.
    type Returns: Send = ();
    /// The [ActorState] that is passed to all handler functions.
    type State: ActorState<Self> = State<Self>;
    /// The address that is returned when this actor is spawned.
    type Address: FromAddress<Self> = Address<Self>;

    type Inbox: InboxType = Unbounded;

    const INBOX_SIZE: usize = 0;

    /// Spawn this [Actor], and return an [Address] of this [Actor].
    fn spawn(self) -> (ActorChild<Self>, Self::Address) {
        // create the packet senders
        let (sender, receiver) = PacketSender::new();
        // create the address from the sender
        let address = Address { sender };
        // create the return address from this raw address
        let actual_address = <Self::Address as FromAddress<Self>>::from_address(address.clone());
        // create the state
        let state = Self::State::starting(actual_address.clone());
        // start the actor
        let child = RunningActor::new(self, state, receiver).run(address);
        // return the address
        (child, actual_address)
    }

    /// This is called when this [Actor] starts, and is executed on a new tokio `Task`
    fn starting(&mut self, state: &mut Self::State) -> MsgFlow<Self> {
        MsgFlow::Ok
    }

    /// This is called when this [Actor] exits.
    fn exiting(self, state: &mut Self::State, exit: Exiting<Self>) -> Self::Returns;
}

//-------------------------------------
// RunningActor
//-------------------------------------

struct RunningActor<A: Actor<State = S>, S: ActorState<A>> {
    actor: A,
    state: S,
    receiver: PacketReceiver<A>,
}

trait PacketFut<A: Actor>: Future<Output = Option<Packet<A>>> + Send {}

impl<A: Actor<State = S> + Send + 'static, S: ActorState<A> + Send + 'static> RunningActor<A, S> {
    pub fn new(actor: A, state: S, receiver: PacketReceiver<A>) -> Self {
        Self {
            actor,
            state,
            receiver,
        }
    }

    pub fn run(self, address: Address<A>) -> ActorChild<A> {
        let handle = tokio::task::spawn(async move { Self::started(self).await });
        ActorChild::new(handle, address)
    }

    async fn started(mut self) -> <A as Actor>::Returns {
        //-------------------------------------
        // Initialization
        //-------------------------------------
        let mut optional_before: Option<Before<A>> = None;
        let mut global_state = Some(self.state);
        let mut global_receiver_stream = Some(
            UnboundedReceiverStream::new(self.receiver.receiver)
                .map(|packet| StreamOutput::Packet(packet)),
        );

        // Execute starting method of actor
        match self.actor.starting(&mut global_state.as_mut().unwrap()) {
            MsgFlow::Ok => (),
            MsgFlow::Before(handler) => optional_before = Some(handler),
            MsgFlow::ErrorExit(error) => {
                return self
                    .actor
                    .exiting(&mut global_state.as_mut().unwrap(), Exiting::Error(error));
            }
            MsgFlow::NormalExit(normal) => {
                return self
                    .actor
                    .exiting(&mut global_state.as_mut().unwrap(), Exiting::Normal(normal));
            }
        }

        //-------------------------------------
        // Loop
        //-------------------------------------
        let res = loop {
            // Create a new combined stream
            let mut combined_stream = stream::select_with_strategy(
                global_state.take().unwrap(),
                global_receiver_stream.take().unwrap(),
                left_first,
            );

            // Take a single value from it
            let next_val = combined_stream.next().await;

            // Destroy the combined stream again
            let (temp_state, temp_receiver_stream) = combined_stream.into_inner();
            global_state = Some(temp_state);
            global_receiver_stream = Some(temp_receiver_stream);

            // Check if Before<A> is set
            if let Some(handler) = optional_before.take() {
                match handler
                    .handle(&mut self.actor, &mut global_state.as_mut().unwrap())
                    .await
                {
                    Flow::Ok => (),
                    Flow::ErrorExit(error) => break Exiting::Error(error),
                    Flow::NormalExit(normal) => break Exiting::Normal(normal),
                }
            }

            // Handle the value from the combined stream
            let flow = match next_val {
                Some(StreamOutput::Scheduled(scheduled)) => {
                    scheduled
                        .handle(&mut self.actor, &mut global_state.as_mut().unwrap())
                        .await
                }
                Some(StreamOutput::Packet(packet)) => {
                    packet
                        .handle(&mut self.actor, &mut global_state.as_mut().unwrap())
                        .await
                }
                None => unreachable!("Addresses should never all be dropped"),
            };

            // Handle the flow returned from handling the value
            match flow {
                InternalFlow::Ok => (),
                InternalFlow::After(handler) => {
                    match handler
                        .handle(&mut self.actor, &mut global_state.as_mut().unwrap())
                        .await
                    {
                        MsgFlow::Ok => (),
                        MsgFlow::Before(handler) => optional_before = Some(handler),
                        MsgFlow::ErrorExit(error) => break Exiting::Error(error),
                        MsgFlow::NormalExit(normal) => break Exiting::Normal(normal),
                    }
                }
                InternalFlow::Before(handler) => optional_before = Some(handler),
                InternalFlow::ErrorExit(error) => break Exiting::Error(error),
                InternalFlow::NormalExit(normal) => break Exiting::Normal(normal),
            };
        };

        //-------------------------------------
        // Exiting
        //-------------------------------------
        self.actor.exiting(&mut global_state.as_mut().unwrap(), res)
    }
}

fn left_first(_: &mut ()) -> PollNext {
    PollNext::Left
}

pub enum StreamOutput<A: Actor> {
    Packet(Packet<A>),
    Scheduled(Scheduled<A>),
}

//-------------------------------------
// Child
//-------------------------------------

pub struct ActorChild<A: Actor> {
    handle: tokio::task::JoinHandle<<A as Actor>::Returns>,
    pub(crate) address: Address<A>,
}

impl<A: Actor> ActorChild<A> {
    pub(crate) fn new(
        handle: tokio::task::JoinHandle<<A as Actor>::Returns>,
        address: Address<A>,
    ) -> Self {
        Self { handle, address }
    }
}

//-------------------------------------
// ActorExit
//-------------------------------------

pub enum Exiting<A: Actor> {
    Error(A::ErrorExit),
    Normal(A::NormalExit),
}

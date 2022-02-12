use std::time::Duration;

use futures::{
    future,
    stream::{self, PollNext},
    StreamExt,
};

use crate::{
    abort::{AbortReceiver, AbortSender, ToAbort},
    address::{Address, RawAddress},
    flows::{ExitFlow, InitFlow, InternalFlow, MsgFlow},
    packet::{Packet},
    process::Process,
    state::{ActorState, State, StreamItem},
    AnyhowError, inbox::{Unbounded, Capacity, packet_channel, PacketReceiver},
};

//--------------------------------------------------------------------------------------------------
//  Actor trait
//--------------------------------------------------------------------------------------------------

/// This is the main trait, which `Zestors` revolves around. All actors have to implement this.
/// if this trait is implemented, it can be spawned with either [spawn]. (or [Spawn::spawn] if
/// [Actor::Init] == [Actor]). See module documentation ([crate]) for more general
/// information.
///
/// ### Methods:
/// There are two required methods which must be implemented manually: [Actor::init] and
/// [Actor::handle_exit].
///
/// [Actor::init] is called whenever the actor has just spawned on a new
/// `tokio::task`. This method should take an [Actor::Init], and then return the [Actor] itself.
///
/// [Actor::handle_exit] is called whenever this actor is trying to exit. This method can decide
/// what to do based on different [ExitReason]s, see the documentation of [ExitReason] for what
/// these reasons can be. **Important**: This method will **not** be called if this [Actor] is
/// exiting because of a `panic` or `hard-abort`.
///
/// ### Types:
/// There are a bunch of associated types, which all have sensible defaults, but can be overridden
/// when necessary. The following types can be overridden: [Actor::ExitError], [Actor::ExitNormal],
/// [Actor::ExitWith], [Actor::State], [Actor::Address], [Actor::Inbox].
///
/// ### Generics:
/// There are two associated generics, which can be used to customize an actor. These also have
/// sensible defaults, but can be overridden: [Actor::INBOX_CAPACITY] and [Actor::ABORT_TIMER].
///
/// ### Basic implementation:
/// ```
/// # use zestors::{InitFlow, ExitFlow, ExitReason, Actor, Spawn, spawn};
/// #
/// struct MyActor {}
///
/// impl Actor for MyActor {
///     fn init(init: Self::Init, state: &mut Self::State) -> InitFlow<Self> {
///         InitFlow::Init(init)
///     }
///
///     fn handle_exit(self, state: &mut Self::State, reason: ExitReason<Self>) -> ExitFlow<Self> {
///         ExitFlow::ContinueExit(reason)
///     }
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let (process, address) = spawn::<MyActor>(MyActor{});
///     // or: let (process, address) = MyActor{}.spawn();
/// }
///
pub trait Actor: Send + Sync + 'static + Sized {
    /// The value that can be used to initialise this actor. This value is then passed on to
    /// [Actor::init], which initialises the actor and returns a `Self`.
    ///
    /// This value is also used to [spawn] this actor.
    ///
    /// The default value for this is `Self`, which means that first `Self` has to be created,
    /// and then the actor can be spawned with this value.
    type Init: Send = Self;

    /// The error type that can be used to stop this [Actor]. [Flow], [ReqFlow] or [InitFlow] can
    /// all propagate this error by applying `?` in a function with these return types.
    /// [Actor::handle_exit] is then called with this error value.
    ///
    /// The default value is [AnyhowError], which is just a type alias for [anyhow::Error]. This
    /// makes it easy to apply `?` to any error type within a function, to make the actor exit.
    type ExitError: Send = AnyhowError;

    /// The type that is used to stop this [Actor] normally. [Flow], [ReqFlow] or [InitFlow] can
    /// return this type. [Actor::handle_exit] is then called with this value.
    ///
    /// The default value for this is `()`.
    type ExitNormal: Send = ();

    /// The type that is returned when this [Actor] exits. This value is the return type of
    /// [Actor::handle_exit].
    ///
    /// The default value for this is [ExitReason]. This means that [Actor::handle_exit] can
    /// directly pass on the [ExitReason] to return type of the [Process] of this actor.
    type ExitWith: Send = ExitReason<Self>;

    /// The [ActorState] that is passed to all handler functions.
    ///
    /// For 99% of applications, the default state of [State] should work perfectly fine. The state
    /// can be used to schedule futures, or get the [Actor::Address] of your own actor.
    type State: ActorState<Self> = State<Self>;

    /// The address that is returned when this actor is spawned.
    ///
    /// The default value for this is `Address<Self>`, which works fine for sending [Msg]s or [Req]sts.
    /// However, since [Address] is not defined in your local crate, it's impossible to directly
    /// implement methods on this. Therefore, we allow you to pass in a custom address.
    ///
    /// This struct must implement [From<Address<Self>>] and [RawAddress<Actor = Self>] in order to be
    /// directly usable as a [Actor::Address].
    /// 
    /// Custom addresses can be derived using [crate::derive::Address], and
    /// optionally [crate::derive::Addressable].
    type Address: From<Address<Self>> + RawAddress<Actor = Self> + Send = Address<Self>;

    /// Whether the inbox should be [Bounded] or [Unbounded]. If setting this to [Bounded],
    /// don't forget to set [Actor::INBOX_CAPACITY] as well.
    ///
    /// The default value for this is [Unbounded].
    type Inbox: Capacity<Self::Inbox> = Unbounded;

    /// If the [Actor::Inbox] is [Bounded], this will be the inbox capacity. If [Unbounded], this
    /// value does not do anything.
    ///
    /// The default for this value is `0`.
    const INBOX_CAPACITY: usize = 0;

    /// This value determines how long this [Actor] should have before it is shut down if it's
    /// [Process] is dropped. When the associated [Process] is dropped, it will have
    /// [Actor::ABORT_TIMER] time after receiving a `soft-abort`, before receiving being
    /// `hard-abort`ed.
    ///
    /// The default value is 5 seconds.
    const ABORT_TIMER: Duration = Duration::from_secs(5);

    /// This method is called whenever this actor starts. [spawn]ing an actor takes as a parameter
    /// the [Actor::Init] value. After a new `tokio::task` is spawned, this function is called with
    /// the parameters passed to [spawn].
    ///
    /// This function returns an [InitFlow], which can either initialise the actor with `Self`, or it
    /// can cancel initialisation and return an error.
    ///
    /// Cancelation does **not** call [Actor::handle_exit], but directly exits the process instead.
    fn init(init: Self::Init, state: &mut Self::State) -> InitFlow<Self>;

    /// Whenever this actor exits for any [ExitReason] (excluding `panics` or `hard-aborts`),
    /// this function is called with the reason. It is possible to either continue the exit,
    /// or resume execution instead.
    fn handle_exit(self, state: &mut Self::State, reason: ExitReason<Self>) -> ExitFlow<Self>;
}

pub trait FromAddress<A: Actor> {
    fn from_raw(address: Address<A>) -> Self;
    fn from_raw_ref(address: &Address<A>) -> &Self;
}

impl<'a, A: Actor> FromAddress<A> for Address<A> {
    fn from_raw(address: Address<A>) -> Self {
        address
    }

    fn from_raw_ref(address: &Address<A>) -> &Self {
        address
    }
}

//--------------------------------------------------------------------------------------------------
//  Spawn
//--------------------------------------------------------------------------------------------------

/// Spawn an [Actor] on a new `tokio::task`. Usage:
/// ```ignore
/// spawn::<MyActor>(init);
/// ```
pub fn spawn<A: Actor>(init: A::Init) -> (Process<A>, A::Address) {
    // Get the capacity for the channel.
    let capacity = <A::Inbox as Capacity<A::Inbox>>::capacity(A::INBOX_CAPACITY);
    // Create the sender and receiver.
    let (sender, receiver) = packet_channel(capacity);
    // Create the raw address from this sender.
    let raw_address = Address::new(sender);
    // create the abort sender
    let (abort_sender, abort_receiver) = AbortSender::new();
    // create the state
    let state = A::State::starting(raw_address.clone().into());
    // spawn the task
    let handle = tokio::task::spawn(event_loop(init, state, receiver, abort_receiver));
    // wrap handle in a new process
    let process = Process::new(handle, raw_address.clone().into(), abort_sender, true);
    // return the process and the address
    (process, raw_address.into())
}

pub trait Spawn: Actor<Init = Self> {
    /// Spawn an actor for which [Actor::Init] == `Self`. See [spawn] documentation for more
    /// information
    fn spawn(self) -> (Process<Self>, Self::Address);
}

impl<A: Actor<Init = Self>> Spawn for A {
    fn spawn(self) -> (Process<Self>, Self::Address) {
        spawn::<A>(self)
    }
}

//--------------------------------------------------------------------------------------------------
//  ExitReason
//--------------------------------------------------------------------------------------------------

/// The exit reason which is passed along to [Actor::handle_exit]. The reason for an exit can be
/// either [ExitReason::Error], [ExitReason::Normal] or [ExitReason::SoftAbort].
///
/// If the actor `panic`s or is `hard-abort`ed, then this function will **not** be called, but the
/// process exits directly.
pub enum ExitReason<A: Actor> {
    /// A `Flow` has returned an error variant.
    Error(A::ExitError),
    /// A `Flow` has returned an exit_normal variant.
    Normal(A::ExitNormal),
    /// The soft_abort message has been received. This can only be done once per [Process].
    SoftAbort,
}

//--------------------------------------------------------------------------------------------------
//  event_loop
//--------------------------------------------------------------------------------------------------

async fn event_loop<A: Actor<State = S>, S: ActorState<A>>(
    init: A::Init,
    mut state: S,
    mut receiver: PacketReceiver<A>,
    abort_receiver: AbortReceiver,
) -> InternalExitReason<A> {
    //-------------------------------------
    // Initialisation
    //-------------------------------------
    let mut optional_abort_receiver = Some(abort_receiver);
    let mut optional_before = None;
    let mut optional_after = None;
    let mut actor = match <A as Actor>::init(init, &mut state) {
        InitFlow::Init(actor) => actor,
        InitFlow::InitAndBefore(actor, action) => {
            optional_before = Some(action);
            actor
        }
        InitFlow::Exit => return InternalExitReason::InitFailed,
    };

    loop {
        //-------------------------------------
        // Wait for next thing to do
        //-------------------------------------
        let (temp_state, temp_receiver, loop_output) =
            next_event(&mut optional_abort_receiver, state, receiver).await;
        state = temp_state;
        receiver = temp_receiver;

        //-------------------------------------
        // Handle before
        //-------------------------------------
        if let Some(action) = optional_before.take() {
            match action.handle(&mut actor, &mut state).await {
                MsgFlow::Ok => (),
                MsgFlow::Before(_) => (), // ignore before from before
                MsgFlow::ExitWithError(e) => {
                    match <A as Actor>::handle_exit(actor, &mut state, ExitReason::Error(e)) {
                        ExitFlow::ContinueExit(exit) => return InternalExitReason::Handled(exit),
                        ExitFlow::Resume(temp_actor) => actor = temp_actor,
                        ExitFlow::ResumeAndBefore(temp_actor, action) => {
                            actor = temp_actor;
                            optional_before = Some(action);
                        }
                    }
                }
                MsgFlow::NormalExit(n) => {
                    match <A as Actor>::handle_exit(actor, &mut state, ExitReason::Normal(n)) {
                        ExitFlow::ContinueExit(exit) => return InternalExitReason::Handled(exit),
                        ExitFlow::Resume(temp_actor) => actor = temp_actor,
                        ExitFlow::ResumeAndBefore(temp_actor, action) => {
                            actor = temp_actor;
                            optional_before = Some(action);
                        }
                    }
                }
            };
        };

        //-------------------------------------
        // Do the thing
        //-------------------------------------
        match loop_output {
            NextEvent::Stream(CombinedStreamOutput::Packet(packet)) => {
                match packet.handle(&mut actor, &mut state).await {
                    InternalFlow::Ok => (),
                    InternalFlow::After(action) => optional_after = Some(action),
                    InternalFlow::Before(action) => optional_before = Some(action),
                    InternalFlow::ErrorExit(e) => {
                        match <A as Actor>::handle_exit(actor, &mut state, ExitReason::Error(e)) {
                            ExitFlow::ContinueExit(exit) => {
                                return InternalExitReason::Handled(exit)
                            }
                            ExitFlow::Resume(temp_actor) => actor = temp_actor,
                            ExitFlow::ResumeAndBefore(temp_actor, action) => {
                                actor = temp_actor;
                                optional_before = Some(action);
                            }
                        }
                    }
                    InternalFlow::NormalExit(n) => {
                        match <A as Actor>::handle_exit(actor, &mut state, ExitReason::Normal(n)) {
                            ExitFlow::ContinueExit(exit) => {
                                return InternalExitReason::Handled(exit)
                            }
                            ExitFlow::Resume(temp_actor) => actor = temp_actor,
                            ExitFlow::ResumeAndBefore(temp_actor, action) => {
                                actor = temp_actor;
                                optional_before = Some(action);
                            }
                        }
                    }
                }
            }
            NextEvent::Stream(CombinedStreamOutput::Scheduled(scheduled)) => {
                match scheduled
                    .handle(&mut actor, &mut state)
                    .await
                    .into_internal()
                {
                    InternalFlow::Ok => (),
                    InternalFlow::After(action) => optional_after = Some(action),
                    InternalFlow::Before(action) => optional_before = Some(action),
                    InternalFlow::ErrorExit(e) => {
                        match <A as Actor>::handle_exit(actor, &mut state, ExitReason::Error(e)) {
                            ExitFlow::ContinueExit(exit) => {
                                return InternalExitReason::Handled(exit)
                            }
                            ExitFlow::Resume(temp_actor) => actor = temp_actor,
                            ExitFlow::ResumeAndBefore(temp_actor, action) => {
                                actor = temp_actor;
                                optional_before = Some(action);
                            }
                        }
                    }
                    InternalFlow::NormalExit(n) => {
                        match <A as Actor>::handle_exit(actor, &mut state, ExitReason::Normal(n)) {
                            ExitFlow::ContinueExit(exit) => {
                                return InternalExitReason::Handled(exit)
                            }
                            ExitFlow::Resume(temp_actor) => actor = temp_actor,
                            ExitFlow::ResumeAndBefore(temp_actor, action) => {
                                actor = temp_actor;
                                optional_before = Some(action);
                            }
                        }
                    }
                }
            }
            NextEvent::SoftAbort => {
                match <A as Actor>::handle_exit(actor, &mut state, ExitReason::SoftAbort) {
                    ExitFlow::ContinueExit(exit) => return InternalExitReason::Handled(exit),
                    ExitFlow::Resume(temp_actor) => actor = temp_actor,
                    ExitFlow::ResumeAndBefore(temp_actor, action) => {
                        actor = temp_actor;
                        optional_before = Some(action);
                    }
                }
            }
        };

        //-------------------------------------
        // Handle after
        //-------------------------------------
        if let Some(action) = optional_after.take() {
            match action.handle(&mut actor, &mut state).await {
                MsgFlow::Ok => (),
                MsgFlow::Before(_) => (), // ignore before from before
                MsgFlow::ExitWithError(e) => {
                    match <A as Actor>::handle_exit(actor, &mut state, ExitReason::Error(e)) {
                        ExitFlow::ContinueExit(exit) => return InternalExitReason::Handled(exit),
                        ExitFlow::Resume(temp_actor) => actor = temp_actor,
                        ExitFlow::ResumeAndBefore(temp_actor, action) => {
                            actor = temp_actor;
                            optional_before = Some(action);
                        }
                    }
                }
                MsgFlow::NormalExit(n) => {
                    match <A as Actor>::handle_exit(actor, &mut state, ExitReason::Normal(n)) {
                        ExitFlow::ContinueExit(exit) => return InternalExitReason::Handled(exit),
                        ExitFlow::Resume(temp_actor) => actor = temp_actor,
                        ExitFlow::ResumeAndBefore(temp_actor, action) => {
                            actor = temp_actor;
                            optional_before = Some(action);
                        }
                    }
                }
            };
        };
    }
}

//--------------------------------------------------------------------------------------------------
//  next_event
//--------------------------------------------------------------------------------------------------

// return the next event which should be handled by this actor.
async fn next_event<A: Actor<State = S>, S: ActorState<A>>(
    optional_abort_receiver: &mut Option<AbortReceiver>,
    state: S,
    receiver: PacketReceiver<A>,
) -> (S, PacketReceiver<A>, NextEvent<A, S>) {
    let mut combined_stream = stream::select_with_strategy(
        state.map(|scheduled| CombinedStreamOutput::Scheduled(scheduled)),
        receiver.map(|packet| CombinedStreamOutput::Packet(packet)),
        left_first_strat,
    );

    let next_val = match optional_abort_receiver.take() {
        // There is still an abort receiver
        Some(abort_receiver) => {
            match future::select(abort_receiver, combined_stream.next()).await {
                future::Either::Right((next_val, abort_receiver)) => {
                    optional_abort_receiver.replace(abort_receiver);
                    NextEvent::Stream(next_val.expect("Addresses should never all be dropped"))
                }
                future::Either::Left((to_abort, next)) => match to_abort {
                    // Process has been aborted
                    ToAbort::Abort => {
                        drop(next);
                        NextEvent::SoftAbort
                    }
                    // Process has been detatched
                    ToAbort::Detatch => NextEvent::Stream(
                        next.await.expect("Addresses should never all be dropped"),
                    ),
                },
            }
        }
        // Abort receiver has already been consumed
        None => NextEvent::Stream(
            combined_stream
                .next()
                .await
                .expect("Addresses should never all be dropped"),
        ),
    };

    // Destroy the combined stream again
    let (state, receiver) = combined_stream.into_inner();
    (state.into_inner(), receiver.into_inner(), next_val)
}

//--------------------------------------------------------------------------------------------------
//  Helper types
//--------------------------------------------------------------------------------------------------

/// The next event that should be handled by the actor.
enum NextEvent<A: Actor<State = S>, S: ActorState<A>> {
    Stream(CombinedStreamOutput<A>),
    SoftAbort,
}

/// Strategy for selecting the left stream
fn left_first_strat(_: &mut ()) -> PollNext {
    PollNext::Left
}

/// The output of the combined stream
enum CombinedStreamOutput<A: Actor> {
    Packet(Packet<A>),
    Scheduled(StreamItem<A>),
}

/// An internal exitreason, which is then converted into the actual exit reason upon awaiting a
/// process.
pub(crate) enum InternalExitReason<A: Actor> {
    InitFailed,
    Handled(A::ExitWith),
}

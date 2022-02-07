use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use futures::{
    future,
    stream::{self, Map, Next, PollNext, SelectWithStrategy},
    Future, FutureExt, StreamExt,
};

use crate::{
    abort::{AbortReceiver, AbortSender, ToAbort},
    action::Action,
    address::{Address, Addressable},
    flows::{ExitFlow, Flow, InitFlow, InternalFlow},
    messaging::{NewInbox, PacketReceiver, PacketSender},
    packets::{HandlerFn, Packet, Unbounded},
    process::Process,
    state::{self, BasicState, State, StateStreamItem},
    AnyhowError,
};

/// This is the main trait, which `Zestors` revolves around. All actors have to implement this.
///
/// There is only one required method which needs to be implemented: `exiting()`, all other methods
/// have sensible defaults. It is possible to override these types and methods one at a time,
/// as is necessary.
///
/// The following is the simplest actor implementation:
/// ```ignore
/// impl Actor for MyActor {
///     fn exit(self, state: &mut Self::State, exit: Exiting<Self>) -> Self::Returns {
///         ()
///     }
///
///     fn initialise(init: Self::Init, state: &mut Self::State) -> InitFlow<Self> {
///         InitFlow::Init(init)
///     }
/// }
/// ```
///
/// ## Overrideable methods:
/// ```ignore
/// /// The error type that can be used to stop this [Actor].
/// type ExitError = AnyhowError;
///
/// /// The type that is used to stop this [Actor] normally.
/// type ExitNormal = ();
///
/// /// The type that is returned when this [Actor] exits.
/// type Returns: Send = ();
///
/// /// The [ActorState] that is passed to all handler functions.
/// type State: ActorState<Self> = State<Self>;
///
/// /// The address that is returned when this actor is spawned.
/// type Address: FromAddress<Self> = Address<Self>;
///
/// /// Whether the inbox should be [Bounded] or [Unbounded].
/// type Inbox: NewInbox<Self, Self::Inbox> = Unbounded;
///
/// /// If the inbox is [Bounded], this will be the inbox capacity
/// const INBOX_CAP: usize = 0;
///
/// /// What this actor should be initialized with
/// type Init = Self;
///
/// /// Spawn this [Actor], and return an [Address] of this [Actor].
/// /// This is called when this [Actor] starts, and is executed on a new tokio `Task`
/// fn initialise(init: Self::Init, state: &mut Self::State) -> InitFlow<Self>;
///
/// /// This is called when this [Actor] exits.
/// fn handle_exit(self, state: &mut Self::State, exit: HandleExit<Self>) -> Self::Returns;
///
/// ```
///
pub trait Actor: Send + Sync + 'static + Sized {
    /// The error type that can be used to stop this [Actor].
    type ExitError: Send = AnyhowError;
    /// The type that is used to stop this [Actor] normally.
    type ExitNormal: Send = ();
    /// The type that is returned when this [Actor] exits.
    type Exit: Send = ();
    /// The [ActorState] that is passed to all handler functions.
    type State: State<Self> = BasicState<Self>;
    /// The address that is returned when this actor is spawned.
    type Address: From<Address<Self>> + Addressable<Self> + Send + Clone = Address<Self>;
    /// Whether the inbox should be [Bounded] or [Unbounded].
    /// If setting this to [Bounded], don't forget to set `INBOX_CAPACITY` as well.
    type Inbox: NewInbox<Self, Self::Inbox> = Unbounded;
    /// What this actor should be initialized with
    type Init: Send = Self;
    /// If the inbox is [Bounded], this will be the inbox capacity
    const INBOX_CAPACITY: usize = 0;
    /// How long should a process wait by default before hard aborting this actor
    const ABORT_TIMER: Duration = Duration::from_secs(5);

    /// Initialise this actor. This is executed on the spawned `tokio::task`.
    fn init(init: Self::Init, state: &mut Self::State) -> InitFlow<Self>;

    /// This is called when this [Actor] exits.
    fn handle_exit(self, state: &mut Self::State, reason: ExitReason<Self>) -> ExitFlow<Self>;
}

pub fn spawn<A: Actor>(init: A::Init) -> (Process<A>, A::Address) {
    // create the packet senders
    let (sender, receiver) =
        <A::Inbox as NewInbox<A, A::Inbox>>::new_packet_sender(A::INBOX_CAPACITY);

    let (abort_sender, abort_receiver) = AbortSender::new();

    // create the return address from this raw address
    let address = <A::Address as From<Address<A>>>::from(Address::new(sender));
    // create the state
    let state = A::State::starting(address.clone());

    let handle = tokio::task::spawn(spawned_new(init, state, receiver, abort_receiver));

    let child = Process::new(handle, address.clone(), abort_sender, true);

    // return the address
    (child, address)
}

pub trait Spawn: Actor<Init = Self> {
    fn spawn(self) -> (Process<Self>, Self::Address);
}

impl<A: Actor<Init = Self>> Spawn for A {
    fn spawn(self) -> (Process<Self>, Self::Address) {
        spawn::<A>(self)
    }
}

async fn spawned<A: Actor<State = S>, S: State<A>>(
    init: A::Init,
    mut state: S,
    receiver: PacketReceiver<A>,
    abort_receiver: AbortReceiver,
) -> InternalExitReason<A> {
    //-------------------------------------
    // Initialization
    //-------------------------------------
    let mut optional_abort_receiver = Some(abort_receiver);
    let mut optional_before: Option<Action<A>> = None;

    // initialise the actor
    let mut actor = match <A as Actor>::init(init, &mut state) {
        InitFlow::Init(actor) => actor,
        InitFlow::InitAndBefore(actor, before) => {
            optional_before = Some(before);
            actor
        }
        InitFlow::Exit => return InternalExitReason::InitFailed,
    };

    let mut global_state = Some(state);
    let mut global_receiver_stream = Some(receiver);

    //-------------------------------------
    // Loop
    //-------------------------------------
    let res = loop {
        // Create a new combined stream
        let mut combined_stream = stream::select_with_strategy(
            global_state
                .take()
                .unwrap()
                .map(|scheduled| StreamOutput::Scheduled(scheduled)),
            global_receiver_stream.take().unwrap(),
            left_first_strat,
        );

        // Take a single value from it
        let next_val = match optional_abort_receiver.take() {
            // There is still an abort receiver
            Some(abort_receiver) => {
                match future::select(abort_receiver, combined_stream.next()).await {
                    future::Either::Right((next_val, abort_receiver)) => {
                        optional_abort_receiver = Some(abort_receiver);
                        next_val.expect("Addresses should never all be dropped")
                    }
                    future::Either::Left((to_abort, next_val)) => match to_abort {
                        // Process has been aborted
                        ToAbort::Abort => {
                            // Destroy the combined stream again
                            let (temp_state, temp_receiver_stream) = combined_stream.into_inner();
                            global_state = Some(temp_state.into_inner());
                            global_receiver_stream = Some(temp_receiver_stream);
                            break ExitReason::SoftAbort;
                        }
                        // Process is now detatched
                        ToAbort::Detatch => next_val
                            .await
                            .expect("Addresses should never all be dropped"),
                    },
                }
            }
            // Abort receiver has already been consumed
            None => combined_stream
                .next()
                .await
                .expect("Addresses should never all be dropped"),
        };

        // Destroy the combined stream again
        let (temp_state, temp_receiver_stream) = combined_stream.into_inner();
        global_state = Some(temp_state.into_inner());
        global_receiver_stream = Some(temp_receiver_stream);

        // Check if Before<A> is set
        if let Some(handler) = optional_before.take() {
            match handler
                .handle(&mut actor, &mut global_state.as_mut().unwrap())
                .await
            {
                Flow::Ok => (),
                Flow::ExitWithError(error) => break ExitReason::Error(error),
                Flow::NormalExit(normal) => break ExitReason::Normal(normal),
                Flow::Before(_ignore) => (), // ignore before if it was set by before,
            }
        }

        // Handle the value from the combined stream
        let flow = match next_val {
            StreamOutput::Scheduled(action_or_flow) => action_or_flow
                .handle(&mut actor, &mut global_state.as_mut().unwrap())
                .await
                .into_internal(),
            StreamOutput::Packet(packet) => {
                packet
                    .handle(&mut actor, &mut global_state.as_mut().unwrap())
                    .await
            }
        };

        // Handle the flow returned from handling the value
        match flow {
            InternalFlow::Ok => (),
            InternalFlow::After(handler) => {
                match handler
                    .handle(&mut actor, &mut global_state.as_mut().unwrap())
                    .await
                {
                    Flow::Ok => (),
                    Flow::Before(handler) => optional_before = Some(handler),
                    Flow::ExitWithError(error) => break ExitReason::Error(error),
                    Flow::NormalExit(normal) => break ExitReason::Normal(normal),
                }
            }
            InternalFlow::Before(handler) => optional_before = Some(handler),
            InternalFlow::ErrorExit(error) => break ExitReason::Error(error),
            InternalFlow::NormalExit(normal) => break ExitReason::Normal(normal),
        };
    };

    //-------------------------------------
    // Exiting
    //-------------------------------------
    match actor.handle_exit(&mut global_state.as_mut().unwrap(), res) {
        ExitFlow::ContinueExit(handled) => InternalExitReason::Handled(handled),
        ExitFlow::Resume(_) => todo!(),
        ExitFlow::ResumeAndBefore(_, _) => todo!(),
    }
}

async fn spawned_new<A: Actor<State = S>, S: State<A>>(
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
            next_thing_to_do(&mut optional_abort_receiver, state, receiver).await;
        state = temp_state;
        receiver = temp_receiver;

        //-------------------------------------
        // Handle before
        //-------------------------------------
        if let Some(action) = optional_before.take() {
            match action.handle(&mut actor, &mut state).await {
                Flow::Ok => (),
                Flow::Before(_) => (), // ignore before from before
                Flow::ExitWithError(e) => {
                    match <A as Actor>::handle_exit(actor, &mut state, ExitReason::Error(e)) {
                        ExitFlow::ContinueExit(exit) => return InternalExitReason::Handled(exit),
                        ExitFlow::Resume(temp_actor) => actor = temp_actor,
                        ExitFlow::ResumeAndBefore(temp_actor, action) => {
                            actor = temp_actor;
                            optional_before = Some(action);
                        }
                    }
                }
                Flow::NormalExit(n) => {
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
            LoopOutput::Stream(StreamOutput::Packet(packet)) => {
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
            LoopOutput::Stream(StreamOutput::Scheduled(scheduled)) => {
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
            LoopOutput::SoftAbort => {
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
                Flow::Ok => (),
                Flow::Before(_) => (), // ignore before from before
                Flow::ExitWithError(e) => {
                    match <A as Actor>::handle_exit(actor, &mut state, ExitReason::Error(e)) {
                        ExitFlow::ContinueExit(exit) => return InternalExitReason::Handled(exit),
                        ExitFlow::Resume(temp_actor) => actor = temp_actor,
                        ExitFlow::ResumeAndBefore(temp_actor, action) => {
                            actor = temp_actor;
                            optional_before = Some(action);
                        }
                    }
                }
                Flow::NormalExit(n) => {
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

async fn next_thing_to_do<A: Actor<State = S>, S: State<A>>(
    optional_abort_receiver: &mut Option<AbortReceiver>,
    state: S,
    receiver: PacketReceiver<A>,
) -> (S, PacketReceiver<A>, LoopOutput<A, S>) {
    let mut combined_stream = stream::select_with_strategy(
        state.map(|scheduled| StreamOutput::Scheduled(scheduled)),
        receiver,
        left_first_strat,
    );

    let next_val = match optional_abort_receiver.take() {
        // There is still an abort receiver
        Some(abort_receiver) => {
            match future::select(abort_receiver, combined_stream.next()).await {
                future::Either::Right((next_val, abort_receiver)) => {
                    optional_abort_receiver.replace(abort_receiver);
                    LoopOutput::Stream(next_val.expect("Addresses should never all be dropped"))
                }
                future::Either::Left((to_abort, next)) => match to_abort {
                    // Process has been aborted
                    ToAbort::Abort => {
                        drop(next);
                        LoopOutput::SoftAbort
                    }
                    // Process has been detatched
                    ToAbort::Detatch => LoopOutput::Stream(
                        next.await.expect("Addresses should never all be dropped"),
                    ),
                },
            }
        }
        // Abort receiver has already been consumed
        None => LoopOutput::Stream(
            combined_stream
                .next()
                .await
                .expect("Addresses should never all be dropped"),
        ),
    };

    // Destroy the combined stream again
    let (mapped_state, receiver) = combined_stream.into_inner();
    let state = mapped_state.into_inner();
    (state, receiver, next_val)
}

enum LoopOutput<A: Actor<State = S>, S: State<A>> {
    Stream(StreamOutput<A>),
    SoftAbort,
}

type NextValue<'a, A, S> = Next<
    'a,
    SelectWithStrategy<
        Map<S, fn(StateStreamItem<A>) -> StreamOutput<A>>,
        PacketReceiver<A>,
        fn(&mut ()) -> PollNext,
        (),
    >,
>;

fn left_first_strat(_: &mut ()) -> PollNext {
    PollNext::Left
}

pub enum StreamOutput<A: Actor> {
    Packet(Packet<A>),
    Scheduled(StateStreamItem<A>),
}

/// The enum which is passed to the `exiting()` callback.
pub enum ExitReason<A: Actor> {
    /// This actor has exited with an error.
    Error(A::ExitError),
    /// This actor has exited normally.
    Normal(A::ExitNormal),
    /// A soft_abort message has been received.
    SoftAbort,
}

pub(crate) enum InternalExitReason<A: Actor> {
    InitFailed,
    Handled(A::Exit),
}

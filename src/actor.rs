use std::{fmt::Debug, time::Duration};

use futures::{
    future::{self, Fuse},
    stream::{self, Next, PollNext},
    FutureExt, StreamExt,
};

use crate::{
    abort::{AbortAction, AbortReceiver, AbortSender},
    action::Action,
    address::Address,
    child::Child,
    flows::{EventFlow, InitFlow, MsgFlow},
    inbox::{channel, ActionReceiver, Capacity, Unbounded},
    state::{ActorState, State, StreamItem},
    AnyhowError,
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
/// # use zestors::{flows::{InitFlow, ExitFlow}, actor::{ExitReason, Actor, Spawn, spawn}};
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
pub trait Actor: Send + Sync + 'static + Debug {
    /// The value that can be used to initialise this actor. This value is then passed on to
    /// [Actor::init], which initialises the actor and returns a `Self`.
    ///
    /// This value is also used to [spawn] this actor.
    ///
    /// The default value for this is `Self`, which means that first `Self` has to be created,
    /// and then the actor can be spawned with this value.
    type Init: Send;

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
    fn init(init: Self::Init, state: &mut Self::State) -> InitFlow<Self>
    where
        Self: Sized;

    /// Whenever this actor exits for any [ExitReason] (excluding `panics` or `hard-aborts`),
    /// this function is called with the reason.
    fn handle_exit(self, state: Self::State, reason: ExitReason<Self>) -> Self::ExitWith
    where
        Self: Sized;

    /// Whenever the actor is detatched, and the handle is subsequently dropped,
    /// this function is called to inform the actor. This means the the actor is not part of a
    /// supervision tree anymore, nor can if ever become part of one again. It can be
    /// useful to log a message here if this should never happen. (Or `MsgFlow::Exit`)
    ///
    /// By default, the actor just resumes execution as if nothing happened.
    fn handle_isolated(&mut self, state: &mut Self::State) -> MsgFlow<Self>
    where
        Self: Sized,
    {
        MsgFlow::Ok
    }
}

//--------------------------------------------------------------------------------------------------
//  Spawn
//--------------------------------------------------------------------------------------------------

/// Spawn an [Actor] on a new `tokio::task`. Usage:
/// ```ignore
/// spawn::<MyActor>(init);
/// ```
pub fn spawn<A: Actor>(init: A::Init) -> (Child<A>, Address<A>) {
    // Get the capacity for the channel.
    let capacity = <A::Inbox as Capacity<A::Inbox>>::capacity(A::INBOX_CAPACITY);
    // Create the sender and receiver.
    let (sender, receiver) = channel(capacity);
    // Create the raw address from this sender.
    let raw_address = Address::new(sender);
    // create the abort sender
    let (abort_sender, abort_receiver) = AbortSender::new();
    // create the state
    let state = A::State::starting(raw_address.clone().into());
    // spawn the task
    let handle = tokio::task::spawn(event_loop(init, state, receiver, abort_receiver));
    // wrap handle in a new process
    let process = Child::new(handle, raw_address.clone().into(), abort_sender, true);
    // return the process and the address
    (process, raw_address.into())
}

pub trait Spawn: Actor<Init = Self> + Sized {
    /// Spawn an actor for which [Actor::Init] == `Self`. See [spawn] documentation for more
    /// information
    fn spawn(self) -> (Child<Self>, Address<Self>);
}

impl<A: Actor<Init = Self>> Spawn for A {
    fn spawn(self) -> (Child<Self>, Address<A>) {
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
#[derive(Debug)]
pub enum ExitReason<A: Actor + ?Sized> {
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
    mut receiver: ActionReceiver<A>,
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
        // Select the next event
        //-------------------------------------
        let next_event = select(&mut optional_abort_receiver, &mut state, &mut receiver).await;

        //-------------------------------------------------
        //  Handle a possible before
        //-------------------------------------------------
        if let Some(action) = optional_before.take() {
            match action.handle(&mut actor, &mut state).await {
                EventFlow::Ok => (),
                EventFlow::Before(_) => (), // ignore before from before
                EventFlow::ErrorExit(e) => {
                    return InternalExitReason::Handled(<A as Actor>::handle_exit(
                        actor,
                        state,
                        ExitReason::Error(e),
                    ))
                }
                EventFlow::NormalExit(n) => {
                    return InternalExitReason::Handled(<A as Actor>::handle_exit(
                        actor,
                        state,
                        ExitReason::Normal(n),
                    ))
                }
                EventFlow::After(_) => unreachable!(),
            };
        };

        //-------------------------------------------------
        //  Handle the next event
        //-------------------------------------------------
        let flow = match next_event {
            NextEvent::StreamItem(StreamItem::Action(action)) => {
                action.handle(&mut actor, &mut state).await
            }
            NextEvent::StreamItem(StreamItem::Flow(flow)) => flow.into_internal(),
            NextEvent::SoftAbort => {
                return InternalExitReason::Handled(<A as Actor>::handle_exit(
                    actor,
                    state,
                    ExitReason::SoftAbort,
                ))
            }
            NextEvent::Isolated => actor.handle_isolated(&mut state).into_internal(),
        };

        //-------------------------------------------------
        //  Handle the flow from this event
        //-------------------------------------------------
        match flow {
            EventFlow::Ok => (),
            EventFlow::After(action) => optional_after = Some(action),
            EventFlow::Before(action) => optional_before = Some(action),
            EventFlow::ErrorExit(e) => {
                return InternalExitReason::Handled(<A as Actor>::handle_exit(
                    actor,
                    state,
                    ExitReason::Error(e),
                ))
            }
            EventFlow::NormalExit(n) => {
                return InternalExitReason::Handled(<A as Actor>::handle_exit(
                    actor,
                    state,
                    ExitReason::Normal(n),
                ))
            }
        }

        //------------------------------------------------------------------------------------------------
        //  Handle a possible after
        //------------------------------------------------------------------------------------------------
        if let Some(action) = optional_after.take() {
            match action.handle(&mut actor, &mut state).await {
                EventFlow::Ok => (),
                EventFlow::Before(_) => (), // ignore before from before
                EventFlow::ErrorExit(e) => {
                    return InternalExitReason::Handled(<A as Actor>::handle_exit(
                        actor,
                        state,
                        ExitReason::Error(e),
                    ))
                }
                EventFlow::NormalExit(n) => {
                    return InternalExitReason::Handled(<A as Actor>::handle_exit(
                        actor,
                        state,
                        ExitReason::Normal(n),
                    ))
                }
                EventFlow::After(_) => unreachable!(),
            };
        };
    }
}

//--------------------------------------------------------------------------------------------------
//  next_event
//--------------------------------------------------------------------------------------------------

// return the next event which should be handled by this actor.
async fn select<A: Actor<State = S>, S: ActorState<A>>(
    optional_abort_receiver: &mut Option<AbortReceiver>,
    state: &mut S,
    receiver: &mut ActionReceiver<A>,
) -> NextEvent<A, S> {
    if let Some(abort_receiver) = optional_abort_receiver {
        tokio::select! {
            biased;

            to_abort = abort_receiver => {
                // Okay, we can remove the abort_receiver
                optional_abort_receiver.take();

                match to_abort {
                    AbortAction::SoftAbort => {
                        NextEvent::SoftAbort
                    }
                    AbortAction::Isolated => {
                        NextEvent::Isolated
                    }
                }
            }
            item = state.next() => {
                match item {
                    Some(item) => {
                        NextEvent::StreamItem(item)
                    }
                    None => {
                        match select_without_state(receiver, optional_abort_receiver.as_mut().unwrap()).await {
                            NextEvent::SoftAbort => {
                                optional_abort_receiver.take();
                                NextEvent::SoftAbort
                            }
                            action => action
                        }
                    }
                }
            }
            item = receiver.next() => {
                NextEvent::StreamItem(StreamItem::Action(item.unwrap()))
            }
        }
    } else {
        select_without_abort(state, receiver).await
    }
}

async fn select_without_state<A: Actor<State = S>, S: ActorState<A>>(
    receiver: &mut ActionReceiver<A>,
    abort_receiver: &mut AbortReceiver,
) -> NextEvent<A, S> {
    tokio::select! {
        biased;

        to_abort = abort_receiver => {
            match to_abort {
                AbortAction::SoftAbort => {
                    NextEvent::SoftAbort
                }
                AbortAction::Isolated => {
                    NextEvent::Isolated
                }
            }
        }
        item = receiver.next() => {
            NextEvent::StreamItem(StreamItem::Action(item.unwrap()))
        }
    }
}

async fn select_without_abort<A: Actor<State = S>, S: ActorState<A>>(
    state: &mut S,
    receiver: &mut ActionReceiver<A>,
) -> NextEvent<A, S> {
    tokio::select! {
        item = state.next() => {
            match item {
                Some(item) => {
                    NextEvent::StreamItem(item)
                }
                None => {
                    NextEvent::StreamItem(StreamItem::Action(receiver.next().await.unwrap()))
                }
            }
        }
        item = receiver.next() => {
            NextEvent::StreamItem(StreamItem::Action(item.unwrap()))
        }
    }
}

// return the next event which should be handled by this actor.
async fn next_event_old<A: Actor<State = S>, S: ActorState<A>>(
    optional_abort_receiver: &mut Option<AbortReceiver>,
    state: S,
    receiver: ActionReceiver<A>,
) -> (S, ActionReceiver<A>, NextEvent<A, S>) {
    // let mut combined_stream = stream::select_with_strategy(
    //     state.map(|scheduled| CombinedStreamOutput::Scheduled(scheduled)),
    //     receiver.map(|packet| CombinedStreamOutput::ActionMsg(packet)),
    //     left_first_strat,
    // );

    // let next_val = match optional_abort_receiver.take() {
    //     // There is still an abort receiver
    //     Some(abort_receiver) => {
    //         match future::select(abort_receiver, combined_stream.next()).await {
    //             future::Either::Right((next_val, abort_receiver)) => {
    //                 optional_abort_receiver.replace(abort_receiver);
    //                 NextEvent::Stream(next_val.expect("Addresses should never all be dropped"))
    //             }
    //             future::Either::Left((to_abort, next)) => match to_abort {
    //                 // Process has been aborted
    //                 ToAbort::Abort => {
    //                     drop(next);
    //                     NextEvent::SoftAbort
    //                 }
    //                 // Process has been detached
    //                 ToAbort::Detatch => NextEvent::Stream(
    //                     next.await.expect("Addresses should never all be dropped"),
    //                 ),
    //             },
    //         }
    //     }
    //     // Abort receiver has already been consumed
    //     None => NextEvent::Stream(
    //         combined_stream
    //             .next()
    //             .await
    //             .expect("Addresses should never all be dropped"),
    //     ),
    // };

    // // Destroy the combined stream again
    // let (state, receiver) = combined_stream.into_inner();
    // (state.into_inner(), receiver.into_inner(), next_val)
    todo!()
}

//--------------------------------------------------------------------------------------------------
//  Helper types
//--------------------------------------------------------------------------------------------------

/// The next event that should be handled by the actor.
enum NextEvent<A: Actor<State = S>, S: ActorState<A>> {
    StreamItem(StreamItem<A>),
    Isolated,
    SoftAbort,
}

/// Strategy for selecting the left stream
fn left_first_strat(_: &mut ()) -> PollNext {
    PollNext::Left
}

/// The output of the combined stream
enum CombinedStreamOutput<A: Actor> {
    ActionMsg(Action<A>),
    Scheduled(StreamItem<A>),
}

/// An internal exitreason, which is then converted into the actual exit reason upon awaiting a
/// process.
pub(crate) enum InternalExitReason<A: Actor> {
    InitFailed,
    Handled(A::ExitWith),
}

impl<A: Actor> std::fmt::Debug for InternalExitReason<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InitFailed => write!(f, "InitFailed"),
            Self::Handled(arg0) => f.debug_tuple("Handled").finish(),
        }
    }
}

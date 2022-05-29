use std::{
    error::Error,
    marker::PhantomData,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use async_trait::async_trait;
use futures::{Future, Stream, StreamExt};
use tokio::time::Instant;
use uuid::Uuid;

use crate::core::*;

pub type ProcessId = Uuid;

//------------------------------------------------------------------------------------------------
//  Scheduler
//------------------------------------------------------------------------------------------------

pub trait Scheduler: Stream<Item = Action<Self>> + Unpin {}
impl<A> Scheduler for A where A: Stream<Item = Action<Self>> + Unpin {}

//------------------------------------------------------------------------------------------------
//  spawn
//------------------------------------------------------------------------------------------------

pub fn spawn_task<F, T>(future: F) -> Child<T::Output>
where
    F: FnOnce(Rcv<ChildSignal>) -> T,
    T: Future + Send + 'static,
    T::Output: Send + 'static,
{
    let exited = Arc::new(AtomicBool::new(false));
    let (snd, rcv) = Snd::new();
    let handle = tokio::task::spawn(future(rcv));
    Child::new(
        handle,
        ProcessId::new_v4(),
        snd,
        Duration::from_millis(1_000),
        exited,
    )
}

pub fn spawn_actor<A: Actor>(init: A::Init) -> (Child<A::Exit>, A::Addr<Local>) {
    // Create the channel through which messages can be sent.
    let (tx, rx) = async_channel::unbounded();

    // Generate a unique ProcessId
    let process_id = ProcessId::new_v4();

    // Process has not yet exited
    let exited = Arc::new(AtomicBool::new(false));

    // Create a oneshot channel through which a childsignal could be sent.
    let (tx_signal, rx_signal) = Snd::<ChildSignal>::new();

    // Create the local address, and from that the actual address.
    let local_addr = LocalAddr::new(tx, process_id);
    let addr = <A::Addr<Local> as Addressable<A>>::from_addr(local_addr);

    // And the actor inbox.
    let state = State::<A>::new(rx, rx_signal, process_id, exited.clone());

    // Spawn the actor, and create the child from the handle
    let handle = tokio::task::spawn(spawned(state, addr.clone(), init));
    let child = Child::new(
        handle,
        process_id,
        tx_signal,
        Duration::from_millis(1_000),
        exited,
    );

    // And finally return both the child and the address
    (child, addr)
}

async fn spawned<A: Actor>(mut state: State<A>, addr: A::Addr<Local>, init: A::Init) -> A::Exit {
    let mut actor = match A::initialize(init, addr).await {
        InitFlow::Init(actor) => actor,
        InitFlow::Exit(exit) => return exit,
    };

    loop {
        let signal = loop_until_signal(&mut state, &mut actor).await;

        match actor.handle_signal(signal, &mut state) {
            SignalFlow::Resume(res_actor, res_state) => {
                actor = res_actor;
                state = res_state
            }
            SignalFlow::Exit(exit) => break exit,
        }
    }
}

async fn loop_until_signal<A: Actor>(state: &mut State<A>, actor: &mut A) -> Signal<A> {
    loop {
        match next_event(state, actor).await {
            NextEvent::Signal(signal) => break signal,
            NextEvent::Action(action) => match action.handle(actor, state).await {
                Ok(Flow::Cont) => (),
                Ok(Flow::Exit) => break Signal::HandlerExit,
                Err(err) => break Signal::HandlerError(err),
            },
        }
    }
}

async fn next_event<A: Actor>(state: &mut State<A>, actor: &mut A) -> NextEvent<A> {
    let scheduler_enabled = state.scheduler_enabled();
    let inbox_open = !state.is_closed();

    match (scheduler_enabled, inbox_open) {
        // Scheduler + Inbox
        (true, true) => tokio::select! {

            // New event from inbox received
            inbox_msg = state.next() => {
                match inbox_msg {
                    // There was a new message -> Return that message.
                    Some(msg) => msg.into(),
                    // The inbox has been closed and is empty
                    None => NextEvent::Signal(Signal::ClosedAndEmpty)
                }
            }

            // New event from actor received
            action = actor.next() => {
                match action {
                    // new action -> return that action.
                    Some(action) => action.into(),
                    // No more actions -> disable the scheduler and await the inbox
                    None => {
                        state.disable_scheduler();
                        match state.next().await {
                            // There was a new message -> Return that message.
                            Some(msg) => msg.into(),
                            // The inbox has been closed and scheduler disabled -> dead
                            None => NextEvent::Signal(Signal::Dead)
                        }
                    },
                }
            }

        },

        // Scheduler only
        (true, false) => match actor.next().await {
            Some(action) => action.into(),
            None => {
                state.disable_scheduler();
                NextEvent::Signal(Signal::Dead)
            }
        },

        // State only
        (false, true) => match state.next().await {
            Some(msg) => msg.into(),
            None => NextEvent::Signal(Signal::Dead),
        },

        // None remain
        (false, false) => NextEvent::Signal(Signal::Dead),
    }
}

enum NextEvent<A: Actor> {
    Signal(Signal<A>),
    Action(Action<A>),
}

//------------------------------------------------------------------------------------------------
//  Actor
//------------------------------------------------------------------------------------------------

pub trait ActorFor: Sized + 'static {
    type Addr<AT: AddrType>: Addressable<Self, AddrType = AT>;
}

pub trait Process: Send + Sized + 'static {
    /// The value that this `Actor` is intialized with.
    type Init: Send + 'static;

    /// The error that it's handler-functions return.
    type Error: Send + 'static;

    /// The value that this actor will exit with.
    type Exit: Send + 'static;
}

/// The central `Actor` trait, around which `Zestors` revolves.
#[async_trait]
pub trait Actor: Send + Sized + 'static + Scheduler + ActorFor {
    /// The value that this `Actor` is intialized with.
    type Init: Send + 'static;

    /// The error that it's handler-functions return.
    type Error: Send + 'static;

    /// The value that this actor will exit with.
    type Exit: Send + 'static;

    fn scheduler_disabled(&mut self, state: &mut State<Self>) -> FlowResult<Self> {
        Ok(Flow::Cont)
    }

    /// The address of this actor, used to send messages.
    // type Addr<AT: AddrType>: Addressable<Actor = Self, AddrType = AT>;

    /// After the new task is spawned, this function is called to initialize the `Actor`.
    async fn initialize(init: Self::Init, addr: Self::Addr<Local>) -> InitFlow<Self>;

    /// Decide what happens when this actor ends up in a state where it should usually
    /// exit.
    ///
    /// For a standard implementation, it is fine for the actor to continue it's exit,
    /// no matter what the reason is.
    fn handle_signal(self, signal: Signal<Self>, state: &mut State<Self>) -> SignalFlow<Self>;
}

//------------------------------------------------------------------------------------------------
//  Flows
//------------------------------------------------------------------------------------------------

pub type FlowResult<A> = Result<Flow, <A as Actor>::Error>;

/// Any handler function can return a `Flow`. This indicates how the `Actor` should continue
/// after the handler function.
pub enum Flow {
    /// Continue execution
    Cont,
    /// Exit normally
    Exit,
}

/// When an actor receives a `Signal`, it can either:
/// - Resume it's execution.
/// - Exit
pub enum SignalFlow<A: Actor> {
    Resume(A, State<A>),
    Exit(A::Exit),
}

/// When an `Actor` is initialized, it can either:
/// - Initialize itself.
/// - Exit directly if initialization failed.
pub enum InitFlow<A: Actor> {
    Init(A),
    Exit(A::Exit),
}

//------------------------------------------------------------------------------------------------
//  Signal
//------------------------------------------------------------------------------------------------

/// Whenever an actor gets into any of the following states, it will be signaled using the
/// `handle_signal` function. The `Signal` that is received can vary, but in general the
/// actor is intended to exit it's process when in receives any of these `Signal`s.
/// todo: if no more things are added, change from Signal<Actor> to Signal<Error> type
#[derive(Debug)]
pub enum Signal<A: Actor> {
    /// One of the handler functions returned an error.
    HandlerError(A::Error),

    /// One of the handler functions exited normally.
    HandlerExit,

    /// This process has been soft-aborted. It will be hard-aborted at the given time.
    /// The inbox of this process has already been closed, so it will not be receiving
    /// any new messages.
    SoftAbort,

    /// This process has been isolated. It is not part of any supervision tree, and never
    /// will be, since it's `Child` has been detached and dropped.
    Isolated,

    /// The `Inbox` has been closed and is also empty. No new messages can be received.
    /// The actor can still continue execution with any `Action`s that have been scheduled.
    ClosedAndEmpty,

    /// THe `Inbox` is closed and empty. It also does not have any more `Action`s scheduled.
    ///
    /// The actor should always exit when this happens. If it doesn't, the
    /// actor will panic!
    Dead,
}

impl<A: Actor> From<InboxSignal> for Signal<A> {
    fn from(signal: InboxSignal) -> Self {
        match signal {
            InboxSignal::SoftAbort => Signal::SoftAbort,
            InboxSignal::Isolated => Signal::Isolated,
        }
    }
}

impl<A: Actor> From<InboxMsg<A>> for NextEvent<A> {
    fn from(msg: InboxMsg<A>) -> Self {
        match msg {
            InboxMsg::Action(action) => NextEvent::Action(action),
            InboxMsg::Signal(signal) => NextEvent::Signal(signal.into()),
        }
    }
}

impl<A: Actor> From<Action<A>> for NextEvent<A> {
    fn from(action: Action<A>) -> Self {
        NextEvent::Action(action)
    }
}

impl<A: Actor> From<Signal<A>> for NextEvent<A> {
    fn from(signal: Signal<A>) -> Self {
        NextEvent::Signal(signal)
    }
}

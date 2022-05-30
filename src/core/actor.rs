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

// pub fn spawn_task<F, T>(future: F) -> Child<>
// where
//     F: FnOnce(Rcv<ChildSignal>) -> T,
//     T: Future + Send + 'static,
//     T::Output: Send + 'static,
// {
//     let exited = Arc::new(AtomicBool::new(false));
//     let (snd, rcv) = Snd::new();
//     let handle = tokio::task::spawn(future(rcv));
//     Child::new(
//         handle,
//         ProcessId::new_v4(),
//         snd,
//         Duration::from_millis(1_000),
//         exited,
//     )
// }

pub fn spawn_actor<A: Actor>(init: A::Init) -> (Child<A>, A::Addr) {
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
    let addr = <A::Addr as Addressable<A>>::from_addr(local_addr);

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

async fn spawned<A: Actor>(mut state: State<A>, addr: A::Addr, init: A::Init) -> A::Exit {
    let mut actor = match A::initialize(init, addr).await {
        InitFlow::Init(actor) => actor,
        InitFlow::Exit(exit) => return exit,
    };

    loop {
        let signal = loop_until_signal(&mut state, &mut actor).await;

        match actor.handle_event(signal, &mut state) {
            EventFlow::Resume(res_actor) => {
                actor = res_actor;
            }
            EventFlow::Exit(exit) => break exit,
        }
    }
}

async fn loop_until_signal<A: Actor>(state: &mut State<A>, actor: &mut A) -> Event<A> {
    loop {
        match next_event(state, actor).await {
            NextEvent::Event(event) => break event,
            NextEvent::Action(action) => match action.handle(actor, state).await {
                Ok(Flow::Cont) => (),
                Ok(Flow::Halt(reason)) => break Event::Halt(reason),
                Err(err) => break Event::Error(err),
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
                    None => NextEvent::Event(Event::Signal(Signal::ClosedAndEmpty))
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
                            None => NextEvent::Event(Event::Signal(Signal::Dead))
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
                NextEvent::Event(Event::Signal(Signal::Dead))
            }
        },

        // State only
        (false, true) => match state.next().await {
            Some(msg) => msg.into(),
            None => NextEvent::Event(Event::Signal(Signal::Dead)),
        },

        // None remain
        (false, false) => NextEvent::Event(Event::Signal(Signal::Dead)),
    }
}

enum NextEvent<A: Actor> {
    Event(Event<A>),
    Action(Action<A>),
}

//------------------------------------------------------------------------------------------------
//  Actor
//------------------------------------------------------------------------------------------------

pub trait ActorFor<AT: AddrType>: Sized + 'static {
    type Addr: Addressable<Self, AddrType = AT>;
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
pub trait Actor: Send + Sized + 'static + Scheduler + ActorFor<Local> {
    /// The value that this `Actor` is intialized with.
    type Init: Send + 'static;

    /// The error that it's handler-functions return.
    type Error: Send + 'static;

    /// The normal value used when halting the Actor
    type Halt: Send + 'static;

    /// The value that this actor will exit with.
    type Exit: Send + 'static;

    fn scheduler_disabled(&mut self, state: &mut State<Self>) -> FlowResult<Self> {
        Ok(Flow::Cont)
    }

    /// After the new task is spawned, this function is called to initialize the `Actor`.
    async fn initialize(init: Self::Init, addr: Self::Addr) -> InitFlow<Self>;

    /// Decide what happens when this actor ends up in a state where it should usually
    /// exit.
    ///
    /// For a standard implementation, it is fine for the actor to continue it's exit,
    /// no matter what the reason is.
    fn handle_event(self, event: Event<Self>, state: &mut State<Self>) -> EventFlow<Self>;
}

//------------------------------------------------------------------------------------------------
//  Flows
//------------------------------------------------------------------------------------------------

pub type FlowResult<A> = Result<Flow<A>, <A as Actor>::Error>;

/// Any handler function can return a `Flow`. This indicates how the `Actor` should continue
/// after the handler function. It can either:
/// -
pub enum Flow<A: Actor> {
    /// Continue execution.
    Cont,
    /// Halt execution of this actor, and call `handle_event` with the value of `A::Halt`.
    Halt(A::Halt),
}

/// When an actor handles an `Event`, it can either:
/// - Resume it's execution with the `Actor`.
/// - Exit execution, and return the value of `A::Exit`.
#[derive(Debug)]
pub enum EventFlow<A: Actor> {
    /// Resume execution.
    Resume(A),
    /// The Actor will completely exit.
    Exit(A::Exit),
}

/// When an `Actor` is initialized, it can either:
/// - Initialize itself with `Actor`.
/// - Exit directly, with `A::Exit`.
#[derive(Debug)]
pub enum InitFlow<A: Actor> {
    /// Initialize the actor, and start execution.
    Init(A),
    /// - Exit execution, and return the value of `A::Exit`.
    Exit(A::Exit),
}

/// An event is anything that the actor should be notified about.
/// In most cases it is correct to exit for all different event types.
#[derive(Debug)]
pub enum Event<A: Actor> {
    /// A `Signal` signalling something important within actor execution.
    Signal(Signal),
    /// A handler method has returned an error.
    Error(A::Error),
    /// A handler method halted this actor with `A::Halt`.
    Halt(A::Halt),
}

/// A signal that the actor should respond to in some way.
/// In most cases, it's correct to always exit here.
#[derive(Debug, Clone)]
pub enum Signal {
    /// This Actor has been soft-aborted
    SoftAbort,
    /// This Actor has been isoleted. There are no more addresses of this actor, thus the Actor
    /// will never receive new messages.
    Isolated,
    /// This Actor's inbox has been closed and is also empty. This actor will only continue it's
    /// execution with futures still scheduled on it's scheduler.
    ClosedAndEmpty,
    /// This Actor is dead, and will never do anything anymore. It is ALWAYS correct to exit here,
    /// and failure to do so will cause the actor to panic.
    Dead,
}

impl From<InboxSignal> for Signal {
    fn from(signal: InboxSignal) -> Self {
        match signal {
            InboxSignal::SoftAbort => Signal::SoftAbort,
            InboxSignal::Isolated => Signal::Isolated,
        }
    }
}

impl<A: Actor> From<InboxSignal> for Event<A> {
    fn from(signal: InboxSignal) -> Self {
        Event::Signal(signal.into())
    }
}

impl<A: Actor> From<InboxMsg<A>> for NextEvent<A> {
    fn from(msg: InboxMsg<A>) -> Self {
        match msg {
            InboxMsg::Action(action) => NextEvent::Action(action),
            InboxMsg::Signal(signal) => NextEvent::Event(signal.into()),
        }
    }
}

impl<A: Actor> From<Action<A>> for NextEvent<A> {
    fn from(action: Action<A>) -> Self {
        NextEvent::Action(action)
    }
}

impl<A: Actor> From<Signal> for NextEvent<A> {
    fn from(signal: Signal) -> Self {
        NextEvent::Event(Event::Signal(signal))
    }
}

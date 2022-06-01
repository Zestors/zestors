use crate::core::*;
use async_trait::async_trait;
use futures::Stream;

//------------------------------------------------------------------------------------------------
//  Actor
//------------------------------------------------------------------------------------------------

/// The central `Actor` trait, around which `Zestors` revolves.
/// This trait has to be implemented for every actor.
#[async_trait]
pub trait Actor: Send + Sized + 'static + Scheduler + ActorFor<Local> {
    /// The value that this `Actor` is intialized with.
    type Init: Send + 'static;

    /// The Error that the handler functions of this actor use.
    type Error: Send + 'static;

    /// The value used when halting this actor using `Flow::Halt(_)`.
    type Halt: Send + 'static;

    /// The value this actor can exit with, either from `InitFlow::Exit(_)` or
    /// `EventFlow::Exit(_)`.
    type Exit: Send + 'static;

    /// After the new task is spawned, this function is called to initialize the `Actor`.
    /// Initialize therefore gets called already spawned on a new `tokio::task`.
    ///
    /// It's possible to return either:
    /// - `InitFlow::Init(_)`, which indicates successful initialization and starts the
    /// actor.
    /// - `InitFlow::Exit(_)`, which indicates failure during initialization and exits
    /// with the given value.
    async fn initialize(init: Self::Init, addr: Self::Addr) -> InitFlow<Self>;

    /// Decide what happens when this actor ends up in a state where it should usually
    /// exit.
    ///
    /// For a standard implementation, it is fine for the actor to continue it's exit,
    /// no matter what the reason is.
    async fn handle_signal(self, event: Signal<Self>, state: &mut State<Self>) -> SignalFlow<Self>;
}

pub trait ActorFor<AT: AddrType>: Sized + 'static {
    type Addr: Addressable<Self, AddrType = AT>;
}

/// Scheduler needs to be implemented for any actor type. It is automatically implemented for any
/// type which implements `Stream<Item = Action<Self>>`.
///
/// For most use-cases, the `#[derive(Scheduler)]` or `#[derive(NoScheduler)]` should work fine.
///
/// If more control is necessary, then it's possible to implement `Stream` manually.
pub trait Scheduler: Stream<Item = Action<Self>> + Unpin {}
impl<A> Scheduler for A where A: Stream<Item = Action<Self>> + Unpin {}

//------------------------------------------------------------------------------------------------
//  Flows
//------------------------------------------------------------------------------------------------

/// A wrapper for `Result<Flow<A>, A::Error>`.
pub type FlowResult<A> = Result<Flow<A>, <A as Actor>::Error>;

/// Any handler function can return a `Flow`. This indicates how the `Actor` should continue
/// after the handler function. It can either:
/// - Continue execution.
/// - Halt the actor with `A::Halt`.
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
pub enum SignalFlow<A: Actor> {
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

//------------------------------------------------------------------------------------------------
//  Signals
//------------------------------------------------------------------------------------------------

/// An event is anything that the actor should be notified about.
/// In most cases it is correct to exit for all different event types.
#[derive(Debug)]
pub enum Signal<A: Actor> {
    /// A Signal coming from the actor internally.
    Actor(ActorSignal),
    /// A handler method has returned an error.
    Error(A::Error),
    /// A handler method halted this actor with `A::Halt`.
    Halt(A::Halt),
}

/// A signal that the actor should respond to in some way.
/// In most cases, it's correct to always exit here.
#[derive(Debug, Clone)]
pub enum ActorSignal {
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

impl From<InboxSignal> for ActorSignal {
    fn from(signal: InboxSignal) -> Self {
        match signal {
            InboxSignal::SoftAbort => ActorSignal::SoftAbort,
            InboxSignal::Isolated => ActorSignal::Isolated,
        }
    }
}

impl<A: Actor> From<InboxSignal> for Signal<A> {
    fn from(signal: InboxSignal) -> Self {
        Signal::Actor(signal.into())
    }
}

impl<A: Actor> From<ActorSignal> for Signal<A> {
    fn from(a: ActorSignal) -> Self {
        Self::Actor(a)
    }
}

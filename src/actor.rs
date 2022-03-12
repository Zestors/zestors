use std::{
    fmt::Debug,
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use futures::{stream::PollNext, Stream, StreamExt};

use crate::{
    abort::{AbortAction, AbortReceiver, AbortSender},
    action::Action,
    address::Address,
    child::Child,
    context::{StreamItem},
    flows::{EventFlow, InitFlow, MsgFlow},
    inbox::{Inbox, self},
    AnyhowError,
};

static NEXT_PROCESS_ID: NextProcessId = NextProcessId::new();

//--------------------------------------------------------------------------------------------------
//  Actor trait
//--------------------------------------------------------------------------------------------------

#[async_trait::async_trait]
pub trait Actor: Send + 'static + Debug + Sized {
    /// The value that can be used to initialise this actor. This value is then passed on to
    /// [Actor::init], which initialises the actor and returns a `Self`.
    ///
    /// This value is also used to [spawn] this actor.
    ///
    /// The default value for this is `Self`, which means that first `Self` has to be created,
    /// and then the actor can be spawned with this value.
    type Init: Send = Self;

    type InitError: Send = ();

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
    type Context: Stream<Item = StreamItem<Self>> + Unpin + Send;

    // /// Whether the inbox should be [Bounded] or [Unbounded]. If setting this to [Bounded],
    // /// don't forget to set [Actor::INBOX_CAPACITY] as well.
    // ///
    // /// The default value for this is [Unbounded].
    // type Inbox: Capacity<Self::Inbox> = Unbounded;

    // /// If the [Actor::Inbox] is [Bounded], this will be the inbox capacity. If [Unbounded], this
    // /// value does not do anything.
    // ///
    // /// The default for this value is `0`.
    // const INBOX_CAPACITY: usize = 0;

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
    async fn init(init: Self::Init, address: Address<Self>) -> InitFlow<Self>;

    /// Whenever this actor exits for any [ExitReason] (excluding `panics` or `hard-aborts`),
    /// this function is called with the reason.
    async fn exit(self, reason: ExitReason<Self>) -> Self::ExitWith;

    /// Whenever the actor is detatched, and the handle is subsequently dropped,
    /// this function is called to inform the actor. This means the the actor is not part of a
    /// supervision tree anymore, nor can if ever become part of one again. It can be
    /// useful to log a message here if this should never happen. (Or `MsgFlow::Exit`)
    ///
    /// By default, the actor just resumes execution as if nothing happened.
    async fn isolated(&mut self) -> MsgFlow<Self> {
        MsgFlow::Ok
    }

    async fn addresses_dropped(&mut self) -> MsgFlow<Self> {
        MsgFlow::Ok
    }

    fn ctx(&mut self) -> &mut Self::Context;

    fn debug() -> &'static str {
        ""
    }
}

//------------------------------------------------------------------------------------------------
//  ProcessId
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct NextProcessId(AtomicU32);
pub type ProcessId = u32;

impl NextProcessId {
    const fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    fn next(&self) -> ProcessId {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}


//--------------------------------------------------------------------------------------------------
//  Spawn
//--------------------------------------------------------------------------------------------------

/// Spawn an [Actor] on a new `tokio::task`. Usage:
/// ```ignore
/// spawn::<MyActor>(init);
/// ```
pub fn spawn<A: Actor>(init: A::Init) -> Child<A> {
    // Aquire a new process id
    let process_id = NEXT_PROCESS_ID.next();
    // Create the sender and receiver.
    let (sender, receiver) = inbox::channel(None);
    // Create the raw address from this sender.
    let address = Address::new(sender, process_id);
    // create the abort sender
    let (abort_sender, abort_receiver) = AbortSender::new();
    // spawn the task
    let handle = tokio::task::spawn(event_loop(init, receiver, address.clone(), abort_receiver));
    // wrap handle in a new process
    let process = Child::new(handle, address, abort_sender, true);
    // return the process and the address
    process
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
    /// It is impossible for this actor to do anything since all addresses have been dropped,
    /// and there are no actions waiting to be taken.
    Dead,
}

//--------------------------------------------------------------------------------------------------
//  event_loop
//--------------------------------------------------------------------------------------------------

async fn event_loop<A: Actor>(
    init: A::Init,
    inbox: Inbox<A>,
    address: Address<A>,
    abort_receiver: AbortReceiver,
) -> InternalExitReason<A> {
    //-------------------------------------
    // Initialisation
    //-------------------------------------
    let mut optional_abort_receiver = Some(abort_receiver);
    let mut optional_inbox = Some(inbox);
    let mut optional_before = None;
    let mut optional_after = None;
    let mut actor = match <A as Actor>::init(init, address).await {
        InitFlow::Init(actor) => actor,
        InitFlow::InitAndBefore(actor, action) => {
            optional_before = Some(action);
            actor
        }
        InitFlow::Error(e) => return InternalExitReason::InitFailed(e),
        InitFlow::InitAndAfter(actor, action) => {
            optional_after = Some(action);
            actor
        }
    };

    loop {
        //-------------------------------------
        // Select the next event
        //-------------------------------------
        let next_event = next_event(
            &mut optional_abort_receiver,
            &mut optional_inbox,
            actor.ctx(),
        )
        .await;

        //-------------------------------------------------
        //  Handle a possible before
        //-------------------------------------------------
        if let Some(action) = optional_before.take() {
            match action.handle(&mut actor).await {
                EventFlow::Ok => (),
                EventFlow::Before(_) => (), // ignore before from before
                EventFlow::ErrorExit(e) => {
                    return InternalExitReason::Handled(actor.exit(ExitReason::Error(e)).await)
                }
                EventFlow::NormalExit(n) => {
                    return InternalExitReason::Handled(actor.exit(ExitReason::Normal(n)).await)
                }
                EventFlow::After(_) => unreachable!(),
            };
        };

        //-------------------------------------------------
        //  Handle the next event
        //-------------------------------------------------
        let flow = match next_event {
            NextEvent::StreamItem(item) => item.handle(&mut actor).await,
            NextEvent::SoftAbort => {
                return InternalExitReason::Handled(actor.exit(ExitReason::SoftAbort).await)
            }
            NextEvent::Isolated => {
                optional_abort_receiver = None;
                actor.isolated().await.into_event_flow()
            }
            NextEvent::AddressesDropped => {
                optional_inbox = None;
                assert!(optional_abort_receiver.is_none());
                actor.addresses_dropped().await.into_event_flow()
            }
            NextEvent::Dead => {
                return InternalExitReason::Handled(actor.exit(ExitReason::Dead).await)
            }
        };

        //-------------------------------------------------
        //  Handle the flow from this event
        //-------------------------------------------------
        match flow {
            EventFlow::Ok => (),
            EventFlow::After(action) => optional_after = Some(action),
            EventFlow::Before(action) => optional_before = Some(action),
            EventFlow::ErrorExit(e) => {
                return InternalExitReason::Handled(actor.exit(ExitReason::Error(e)).await)
            }
            EventFlow::NormalExit(n) => {
                return InternalExitReason::Handled(actor.exit(ExitReason::Normal(n)).await)
            }
        }

        //------------------------------------------------------------------------------------------------
        //  Handle a possible after
        //------------------------------------------------------------------------------------------------
        if let Some(action) = optional_after.take() {
            match action.handle(&mut actor).await {
                EventFlow::Ok => (),
                EventFlow::Before(_) => (), // ignore before from before
                EventFlow::ErrorExit(e) => {
                    return InternalExitReason::Handled(actor.exit(ExitReason::Error(e)).await)
                }
                EventFlow::NormalExit(n) => {
                    return InternalExitReason::Handled(actor.exit(ExitReason::Normal(n)).await)
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
async fn next_event<A: Actor>(
    optional_abort_receiver: &mut Option<AbortReceiver>,
    optional_inbox: &mut Option<Inbox<A>>,
    ctx: &mut A::Context,
) -> NextEvent<A> {
    match (optional_abort_receiver, optional_inbox) {
        (None, None) => match ctx.next().await {
            Some(event) => event.into(),
            None => NextEvent::Dead,
        },
        (None, Some(inbox)) => select_without_abort(ctx, inbox).await,
        (Some(_abort_receiver), None) => unreachable!("child also has an address!"),
        (Some(abort_receiver), Some(inbox)) => select(abort_receiver, inbox, ctx).await,
    }
}

async fn select<A: Actor>(
    mut abort_receiver: &mut AbortReceiver,
    inbox: &mut Inbox<A>,
    ctx: &mut A::Context,
) -> NextEvent<A> {
    tokio::select! {
        biased;

        to_abort = &mut abort_receiver => {
            to_abort.into()
        }

        action = ctx.next() => {
            match action {
                Some(action) => action.into(),
                None => select_without_ctx(inbox, abort_receiver).await,
            }
        }

        action = inbox.next() => {
            match action {
                Some(action) => action.into(),
                None => NextEvent::AddressesDropped,
            }
        }
    }
}

async fn select_without_ctx<A: Actor>(
    inbox: &mut Inbox<A>,
    abort_receiver: &mut AbortReceiver,
) -> NextEvent<A> {
    tokio::select! {
        biased;

        to_abort = abort_receiver => {
            to_abort.into()
        }

        action = inbox.next() => {
            match action {
                Some(action) => action.into(),
                None => unreachable!("child also has an address!"),
            }
        }
    }
}

async fn select_without_abort<A: Actor>(ctx: &mut A::Context, receiver: &mut Inbox<A>) -> NextEvent<A> {
    tokio::select! {
        biased;

        item = ctx.next() => {
            match item {
                Some(item) => {
                    item.into()
                }
                None => {
                    match receiver.next().await {
                        Some(action) => action.into(),
                        None => NextEvent::Dead,
                    }
                }
            }
        }

        item = receiver.next() => {
            match item {
                Some(item) => item.into(),
                None => NextEvent::AddressesDropped,
            }
        }
    }
}

//--------------------------------------------------------------------------------------------------
//  Helper types
//--------------------------------------------------------------------------------------------------

/// The next event that should be handled by the actor.
enum NextEvent<A: Actor> {
    StreamItem(StreamItem<A>),
    Isolated,
    AddressesDropped,
    SoftAbort,
    Dead,
}

impl<A: Actor> From<StreamItem<A>> for NextEvent<A> {
    fn from(item: StreamItem<A>) -> Self {
        NextEvent::StreamItem(item)
    }
}

impl<A: Actor> From<Action<A>> for NextEvent<A> {
    fn from(action: Action<A>) -> Self {
        NextEvent::StreamItem(StreamItem::Action(action))
    }
}

impl<A: Actor> From<AbortAction> for NextEvent<A> {
    fn from(item: AbortAction) -> Self {
        match item {
            AbortAction::SoftAbort => NextEvent::SoftAbort,
            AbortAction::Isolated => NextEvent::Isolated,
        }
    }
}

/// An internal exitreason, which is then converted into the actual exit reason upon awaiting a
/// process.
pub(crate) enum InternalExitReason<A: Actor> {
    InitFailed(A::InitError),
    Handled(A::ExitWith),
}

impl<A: Actor> std::fmt::Debug for InternalExitReason<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InitFailed(e) => write!(f, "InitFailed"),
            Self::Handled(arg0) => f.debug_tuple("Handled").finish(),
        }
    }
}

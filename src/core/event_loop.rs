use crate::core::*;
use futures::StreamExt;
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

/// Spawn a new actor of type `A`. As it's parameter this takes `A::Init`, the value that this
/// actor is initialized with.
///
/// This function will not wait for the actor to initialize, but will always return successfully.
/// To await a succesful initialization, immeadeately send a message to this actor after spawning.
pub fn spawn_actor<A: Actor>(init: A::Init) -> (Child<A>, A::Addr) {
    // Create the channel through which messages can be sent.
    let (tx, rx) = async_channel::unbounded();
    // Generate a unique ProcessId
    let process_id = ProcessId::new_v4();
    // And from that the shared state
    let shared = Arc::new(SharedProcessData::new(process_id));
    // Create a oneshot channel through which a childsignal could be sent.
    let (tx_signal, rx_signal) = new_channel::<StateSignal>();
    // Create the local address, and from that the actual address.
    let addr = <A::Addr as Addressable<A>>::from_addr(LocalAddr::new(tx, shared.clone()));
    // And the actor inbox.
    let state = State::<A>::new(rx, rx_signal, shared.clone());
    // Spawn the actor, and create the child from the handle
    let handle = tokio::task::spawn(event_loop(state, addr.clone(), init));


    let child = Child::new(
        handle,
        tx_signal,
        Some(Duration::from_millis(1_000)),
        shared
    );
    // And finally return both the child and the address
    (child, addr)
}

//------------------------------------------------------------------------------------------------
//  event_loop
//------------------------------------------------------------------------------------------------

/// The event_loop of an actor. This gets called immeadeately after the actor is spawned.
async fn event_loop<A: Actor>(mut state: State<A>, addr: A::Addr, init: A::Init) -> A::Exit {
    let mut actor = match A::initialize(init, addr).await {
        InitFlow::Init(actor) => actor,
        InitFlow::Exit(exit) => return exit,
    };

    loop {
        let signal = run_until_signal(&mut state, &mut actor).await;

        match actor.handle_signal(signal, &mut state).await {
            SignalFlow::Resume(res_actor) => {
                actor = res_actor;
            }
            SignalFlow::Exit(exit) => break exit,
        }
    }
}

/// Runs the actor until a new event needs to be handled.
async fn run_until_signal<A: Actor>(state: &mut State<A>, actor: &mut A) -> Signal<A> {
    loop {
        match next_event(state, actor).await {
            Event::Signal(signal) => break signal,
            Event::Action(action) => match action.handle(actor, state).await {
                Ok(Flow::Cont) => (),
                Ok(Flow::Halt(reason)) => break Signal::Halt(reason),
                Err(err) => break Signal::Error(err),
            },
        }
    }
}

/// Gets the next event for an actor.
async fn next_event<A: Actor>(state: &mut State<A>, actor: &mut A) -> Event<A> {
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
                    None => Event::Signal(Signal::Actor(ActorSignal::ClosedAndEmpty))
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
                            None => Event::Signal(Signal::Actor(ActorSignal::Dead))
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
                Event::Signal(Signal::Actor(ActorSignal::Dead))
            }
        },

        // State only
        (false, true) => match state.next().await {
            Some(msg) => msg.into(),
            None => Event::Signal(Signal::Actor(ActorSignal::Dead)),
        },

        // None remain
        (false, false) => Event::Signal(Signal::Actor(ActorSignal::Dead)),
    }
}

//------------------------------------------------------------------------------------------------
//  Event
//------------------------------------------------------------------------------------------------

/// Can be either an Signal or an Action.
pub(crate) enum Event<A: Actor> {
    Signal(Signal<A>),
    Action(Action<A>),
}

impl<A: Actor> From<StateMsg<A>> for Event<A> {
    fn from(msg: StateMsg<A>) -> Self {
        match msg {
            StateMsg::Action(action) => Event::Action(action),
            StateMsg::Signal(signal) => Event::Signal(signal.into()),
        }
    }
}

impl<A: Actor> From<Action<A>> for Event<A> {
    fn from(action: Action<A>) -> Self {
        Event::Action(action)
    }
}

impl<A: Actor> From<ActorSignal> for Event<A> {
    fn from(signal: ActorSignal) -> Self {
        Event::Signal(Signal::Actor(signal))
    }
}
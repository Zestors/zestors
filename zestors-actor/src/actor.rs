use crate::*;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use zestors_core::{
    config::Config,
    error::RecvError,
    messaging::Protocol,
    process::{spawn, Address, Child, Inbox},
};

#[async_trait]
pub trait Handler: Actor {
    async fn handle(
        &mut self,
        inbox: &mut Inbox<Self::Protocol>,
        protocol: Self::Protocol,
    ) -> Result<Flow, Self::Error>;
}

#[async_trait]
pub trait Actor: Sized + Send + 'static + Scheduler<Actor = Self> {
    /// What protocol does this actor handle
    type Protocol: Protocol;

    /// What the actor is intialized with
    type Init: Send + 'static;

    /// The value this actor exits with
    type Exit: Send + 'static;

    /// What error can be returned when handling a message
    type Error: Send + 'static;

    /// The config that this actor will be spawned with
    fn config() -> Config;

    async fn init(init: Self::Init, inbox: &mut Inbox<Self::Protocol>) -> InitFlow<Self>;

    async fn exit(
        self,
        inbox: &mut Inbox<Self::Protocol>,
        exception: Exception<Self>,
    ) -> ExitFlow<Self>;
}

#[async_trait]
pub trait Scheduler: Sized + Unpin {
    type Actor: Actor;

    fn poll_next(
        pin: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Event<Self::Actor>>>;

    fn re_enable(&self) -> bool;
}

pub fn spawn_actor<A: Handler>(
    init: A::Init,
) -> (Child<A::Exit, A::Protocol>, Address<A::Protocol>) {
    spawn(A::config(), |inbox| async move {
        event_loop::<A>(init, inbox).await
    })
}

pub enum Exception<A: Actor> {
    Stop,
    CustomError(A::Error),
    Halt,
    ClosedAndEmpty,
}

pub enum Flow {
    Cont,
    Stop,
}

pub enum ExitFlow<A: Actor> {
    Cont(A),
    Exit(A::Exit),
}

pub enum InitFlow<A: Actor> {
    Start(A),
    Exit(A::Exit),
}

impl<A: Actor> From<RecvError> for Exception<A> {
    fn from(e: RecvError) -> Self {
        match e {
            RecvError::Halted => Self::Halt,
            RecvError::ClosedAndEmpty => Self::ClosedAndEmpty,
        }
    }
}

struct EventStream<'a, A: Actor>(pub &'a mut A);

impl<'a, A> Stream for EventStream<'a, A>
where
    A: Actor,
{
    type Item = Event<A>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        A::poll_next(Pin::new(self.0), cx)
    }
}

impl<'a, A: Actor> Unpin for EventStream<'a, A> {}

async fn event_loop<H: Handler>(init: H::Init, mut inbox: Inbox<H::Protocol>) -> H::Exit {
    let mut enabled = true;

    let mut actor = match H::init(init, &mut inbox).await {
        InitFlow::Start(actor) => actor,
        InitFlow::Exit(exit) => return exit,
    };

    loop {
        if !enabled {
            enabled = actor.re_enable();
        }

        if enabled {
            let mut scheduler = EventStream(&mut actor);

            tokio::select! {
                biased;

                action = scheduler.next() => {
                    match action {
                        Some(event) => {
                            actor = match handle_event(event, actor, &mut inbox).await {
                                Ok(actor) => actor,
                                Err(exit) => break exit,
                            }
                        }
                        None => {
                            enabled = false;

                            actor = match handle_recv(inbox.recv().await, actor, &mut inbox).await {
                                Ok(actor) => actor,
                                Err(exit) => break exit,
                            }
                        }
                    }
                }

                msg = inbox.recv() => {
                    actor = match handle_recv(msg, actor, &mut inbox).await {
                        Ok(actor) => actor,
                        Err(exit) => break exit,
                    }
                }

            }
        } else {
            actor = match handle_recv(inbox.recv().await, actor, &mut inbox).await {
                Ok(actor) => actor,
                Err(exit) => break exit,
            }
        }
    }
}

async fn handle_recv<H: Handler>(
    msg: Result<H::Protocol, RecvError>,
    mut actor: H,
    inbox: &mut Inbox<H::Protocol>,
) -> Result<H, H::Exit> {
    match msg {
        Ok(msg) => {
            let exception = match actor.handle(inbox, msg).await {
                Ok(flow) => match flow {
                    Flow::Cont => None,
                    Flow::Stop => Some(Exception::Stop),
                },
                Err(e) => Some(Exception::CustomError(e)),
            };

            if let Some(exception) = exception {
                match actor.exit(inbox, exception).await {
                    ExitFlow::Cont(actor) => Ok(actor),
                    ExitFlow::Exit(exit) => Err(exit),
                }
            } else {
                Ok(actor)
            }
        }

        Err(e) => match actor.exit(inbox, e.into()).await {
            ExitFlow::Cont(actor) => Ok(actor),
            ExitFlow::Exit(exit) => Err(exit),
        },
    }
}

async fn handle_event<H: Handler>(
    event: Event<H>,
    mut actor: H,
    inbox: &mut Inbox<H::Protocol>,
) -> Result<H, H::Exit> {
    let exception = match event.handle(&mut actor, inbox).await {
        Ok(flow) => match flow {
            Flow::Cont => None,
            Flow::Stop => Some(Exception::Stop),
        },
        Err(e) => Some(Exception::CustomError(e)),
    };

    if let Some(exception) = exception {
        match actor.exit(inbox, exception).await {
            ExitFlow::Cont(actor) => Ok(actor),
            ExitFlow::Exit(exit) => Err(exit),
        }
    } else {
        Ok(actor)
    }
}

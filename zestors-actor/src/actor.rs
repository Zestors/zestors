use crate::*;
use async_trait::async_trait;
use futures::{Future, Stream, StreamExt};
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

pub fn spawn_actor<A: Handler>(
    init: A::Init,
) -> (Child<A::Exit, A::Protocol>, Address<A::Protocol>) {
    spawn(
        A::config(),
        |inbox| async move { event_loop::run::<A>(init, inbox).await },
    )
}

#[async_trait]
pub trait Handler: Actor {
    async fn handle(
        &mut self,
        inbox: &mut Inbox<Self::Protocol>,
        protocol: Self::Protocol,
    ) -> Result<Flow, Self::Error>;
}

#[async_trait]
pub trait Actor: Sized + Send + 'static + Scheduler<Self::Protocol> {
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

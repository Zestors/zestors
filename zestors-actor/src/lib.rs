use async_trait::async_trait;
use zestors_core::{
    process::{spawn, Address, Child, Inbox},
    config::Config,
    error::RecvError,
    protocol::Protocol,
};

#[async_trait]
pub trait Actor: Sized + Send + 'static {
    /// What protocol does this actor handle
    type Protocol: Protocol;

    /// What the actor is intialized with
    type Init: Send + 'static;

    /// The value this actor exits with
    type Exit: Send + 'static;

    /// What error can be returned when handling a message
    type Error: Send + 'static;

    async fn init(init: Self::Init, inbox: &mut Inbox<Self::Protocol>) -> Result<Self, Self::Exit>;

    async fn handle_message(
        &mut self,
        msg: Self::Protocol,
        inbox: &mut Inbox<Self::Protocol>,
    ) -> Result<Flow, Self::Error>;

    async fn handle_error(
        &mut self,
        error: ActorError<Self>,
        inbox: &mut Inbox<Self::Protocol>,
    ) -> Result<Self, Self::Exit>;

    fn config() -> Config;
}

pub fn spawn_actor<A: Actor>(init: A::Init) -> (Child<A::Exit, A::Protocol>, Address<A::Protocol>) {
    spawn(A::config(), |inbox| async move {
        spawned::<A>(init, inbox).await
    })
}

pub enum ActorError<A: Actor> {
    Stopped(A::Error),
    Halted,
    ClosedAndEmpty,
}

impl<A: Actor> From<RecvError> for ActorError<A> {
    fn from(e: RecvError) -> Self {
        match e {
            RecvError::Halted => Self::Halted,
            RecvError::ClosedAndEmpty => Self::ClosedAndEmpty,
        }
    }
}

async fn spawned<A: Actor>(init: A::Init, mut inbox: Inbox<A::Protocol>) -> A::Exit {
    let mut actor = match <A as Actor>::init(init, &mut inbox).await {
        Ok(actor) => actor,
        Err(exit) => return exit,
    };

    loop {
        match inbox.recv().await {
            Ok(msg) => match actor.handle_message(msg, &mut inbox).await {
                Ok(flow) => todo!(),
                Err(e) => todo!(),
            },
            Err(error) => {
                actor.handle_error(error.into(), &mut inbox);
            }
        }
    }
}

pub enum Flow {
    Ok,
}

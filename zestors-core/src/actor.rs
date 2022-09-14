use crate::*;
use async_trait::async_trait;

pub trait Actor: Sized {
    type Protocol: Protocol;
    type Init: Send + 'static;
    type Exit: Send + 'static;
    type Runner: Runner<Actor = Self>;
}

#[async_trait]
pub trait Runner {
    type Actor: Actor;

    async fn run(
        init: <Self::Actor as Actor>::Init,
        inbox: &mut Inbox<<Self::Actor as Actor>::Protocol>,
    ) -> <Self::Actor as Actor>::Exit;
}

pub trait ActorExt: Actor {
    fn spawn(
        init: Self::Init,
        config: Config,
    ) -> (Child<Self::Exit, Self::Protocol>, Address<Self::Protocol>);
}

impl<A: Actor> ActorExt for A {
    fn spawn(
        init: Self::Init,
        config: Config,
    ) -> (Child<Self::Exit, Self::Protocol>, Address<Self::Protocol>) {
        spawn(config, |mut inbox| async move {
            <Self::Runner as Runner>::run(init, &mut inbox).await
        })
    }
}

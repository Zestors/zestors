use async_trait::async_trait;

use crate::*;

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

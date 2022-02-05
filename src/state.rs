use std::{pin::Pin, task::Poll};

use futures::{ stream::FuturesUnordered, Future, FutureExt, Stream, StreamExt};

use crate::{
    actor::{Actor, StreamOutput},
    address::Address,
    flow::{Flow, MsgFlow, InternalFlow}, action::{AsyncAction, SyncAction},
};

//-------------------------------------
// ActorState Trait
//-------------------------------------

pub trait ActorState<A: Actor + ?Sized>:
    Send + 'static + Sized + Stream<Item = StreamOutput<A>> + Unpin
{
    fn starting(address: A::Address) -> Self;
    fn this_address(&self) -> &A::Address;
}

//-------------------------------------
// Scheduled
//-------------------------------------

pub enum Scheduled<A: Actor> {
    None(MsgFlow<A>),
    Async(AsyncAction<A, MsgFlow<A>>),
    Sync(SyncAction<A, MsgFlow<A>>)
}

impl<A: Actor> Scheduled<A> {
    pub(crate) async fn handle(self, actor: &mut A, state: &mut A::State) -> InternalFlow<A> {
        match self {
            Scheduled::None(flow) => flow.into_internal(),
            Scheduled::Async(async_action) => async_action
                .handle(actor, state)
                .await
                .into_internal(),
            Scheduled::Sync(sync_action) => sync_action
                .handle(actor, state)
                .into_internal(),
        }
    }
}

//-------------------------------------
// NoState
//-------------------------------------

pub struct LightWeightState<A: Actor + ?Sized> {
    address: A::Address
}

impl<A: Actor> ActorState<A> for LightWeightState<A> {
    fn starting(address: A::Address) -> Self {
        Self {
            address
        }
    }

    fn this_address(&self) -> &A::Address {
        &self.address
    }
}

impl<A: Actor> Stream for LightWeightState<A> {
    type Item = StreamOutput<A>;

    fn poll_next(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }
}

unsafe impl<A: Actor> Send for LightWeightState<A> {}
impl<A: Actor> Unpin for LightWeightState<A> {}


//-------------------------------------
// State
//-------------------------------------

pub struct State<A: Actor + ?Sized> {
    address: A::Address,
    scheduled: FuturesUnordered<Pin<Box<dyn Future<Output = Scheduled<A>>>>>,
}

impl<A: Actor> State<A> {
    pub fn schedule<F: 'static + Future<Output = MsgFlow<A>>>(&mut self, future: F) {
        let future = Box::pin(async move {
            Scheduled::None(future.await)
        });
        self.scheduled.push(future)
    }

    pub fn schedule_and_then<F: Future<Output = P> + 'static, P: Send + 'static>(
        &mut self,
        future: F,
        function: fn(&mut A, &mut A::State, P) -> MsgFlow<A>,
    ) {
        let future = Box::pin(async move {
            let output = future.await;
            Scheduled::Sync(SyncAction::<_, MsgFlow<A>>::new(output, function))
        });
        self.scheduled.push(future)
    }

    pub fn schedule_and_then_async<
        F1: Future<Output = P> + Send + 'static,
        P: Send + 'static,
        F2: Future<Output = MsgFlow<A>> + Send + 'static,
    >(
        &mut self,
        future: F1,
        function: fn(&mut A, &mut A::State, P) -> F2,
    ) {
        let future = Box::pin(async move {
            let output = future.await;
            Scheduled::Async(AsyncAction::<_, MsgFlow<A>>::new(output, function))
        });
        self.scheduled.push(future)
    }
}

impl<A: Actor> ActorState<A> for State<A> {
    fn starting(address: A::Address) -> Self {
        Self {
            address,
            scheduled: FuturesUnordered::new(),
        }
    }

    fn this_address(&self) -> &A::Address {
        &self.address
    }
}

impl<A: Actor> Stream for State<A> {
    type Item = StreamOutput<A>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.scheduled
            .poll_next_unpin(cx)
            .map(|flow| flow.map(|scheduled| StreamOutput::Scheduled(scheduled)))
    }
}

unsafe impl<A: Actor> Send for State<A> {}
impl<A: Actor> Unpin for State<A> {}

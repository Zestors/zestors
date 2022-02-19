use std::{pin::Pin, task::Poll};

use futures::{stream::FuturesUnordered, Future, Stream, StreamExt};

use crate::{
    action::Action,
    actor::Actor,
    address::Address,
    flows::{EventFlow, MsgFlow},
};

//--------------------------------------------------------------------------------------------------
//  State Trait
//--------------------------------------------------------------------------------------------------

/// This is the trait that should be implemented for defining a custom actor state.
///
/// todo:
/// this trait should offer many callbacks.
/// for example, it could receive a notification whenever a msg is handled etc, so that
/// the state can offer things to the user like `time_since_last_msg() -> Duration`.
///
/// todo:
/// it should be possible for this state not to have overhead if scheduling of calls is not
/// necessary.
pub trait ActorState<A: Actor + ?Sized>:
    Send + 'static + Sized + Stream<Item = StreamItem<A>> + Unpin
{
    fn starting(address: Address<A>) -> Self;
    fn address(&self) -> &Address<A>;
}

//--------------------------------------------------------------------------------------------------
//  ActionOrFlow
//--------------------------------------------------------------------------------------------------

/// `Item` of the stream that all actor states should implement
pub enum StreamItem<A: Actor + ?Sized> {
    Action(Action<A>),
    Flow(MsgFlow<A>),
}

impl<A: Actor> StreamItem<A> {
    /// If this is an action, handle it and return the flow.
    /// Otherwise just return the flow directly.
    pub(crate) async fn handle(self, actor: &mut A, state: &mut A::State) -> EventFlow<A> {
        match self {
            StreamItem::Action(action) => action.handle(actor, state).await,
            StreamItem::Flow(flow) => flow.into_internal(),
        }
    }
}

//--------------------------------------------------------------------------------------------------
//  State: the default state implementation
//--------------------------------------------------------------------------------------------------

/// This is the default state implementation, that should be fine for 90% of use cases. It offers
/// a lot of functionality, while still being quite fast.
pub struct State<A: Actor + ?Sized> {
    address: Address<A>,
    scheduled: FuturesUnordered<Pin<Box<dyn Future<Output = StreamItem<A>> + Send>>>,
}

impl<A: Actor + ?Sized> State<A> {
    /// Schedule a future that should be ran on this actor. It will be executed on the actor
    /// with priority over incoming messages.
    pub fn schedule<F: 'static + Future<Output = MsgFlow<A>> + Send>(&mut self, future: F) {
        let future = Box::pin(async move { StreamItem::Flow(future.await) });
        self.scheduled.push(future)
    }

    /// Schedule a future that should be run on this actor. It will be executed on the actor
    /// with priority over incoming messages.
    ///
    /// After the future completes, the function will be handled.
    pub fn schedule_and_then<F: Future<Output = P> + 'static + Send, P: Send + 'static>(
        &mut self,
        future: F,
        function: fn(&mut A, &mut A::State, P) -> MsgFlow<A>,
    ) where
        A: Sized,
    {
        let future = Box::pin(async move {
            let output = future.await;
            StreamItem::Action(Action::new_sync(output, function))
        });
        self.scheduled.push(future)
    }

    /// Same as [State::schedule_and_then], except that it takes an asynchronous function.
    pub fn schedule_and_then_async<
        F1: Future<Output = P> + Send + 'static,
        P: Send + 'static,
        F2: Future<Output = MsgFlow<A>> + Send + 'static,
    >(
        &mut self,
        future: F1,
        function: fn(&mut A, &mut A::State, P) -> F2,
    ) where
        A: Sized,
    {
        let future = Box::pin(async move {
            let output = future.await;
            StreamItem::Action(Action::new_async(output, function))
        });
        self.scheduled.push(future)
    }
}

impl<A: Actor + ?Sized> ActorState<A> for State<A> {
    fn starting(address: Address<A>) -> Self {
        Self {
            address,
            scheduled: FuturesUnordered::new(),
        }
    }

    fn address(&self) -> &Address<A> {
        &self.address
    }
}

impl<A: Actor + ?Sized> Stream for State<A> {
    type Item = StreamItem<A>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        // todo: might be more efficient!
        // match self.scheduled.poll_next_unpin(cx) {
        //     Poll::Ready(None) => Poll::Pending,
        //     other => other,
        // }
        self.scheduled.poll_next_unpin(cx)
    }
}

impl<A: Actor + ?Sized> Unpin for State<A> {}

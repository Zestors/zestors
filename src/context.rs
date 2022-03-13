use std::{fmt::Debug, intrinsics::transmute, marker::PhantomData, pin::Pin, task::Poll};

use futures::{
    stream::{self, FuturesUnordered},
    Future, Stream, StreamExt,
};

use crate::{
    action::Action,
    actor::Actor,
    address::Address,
    flows::{EventFlow, MsgFlow},
    function::MsgFn,
};


//--------------------------------------------------------------------------------------------------
//  StreamItem
//--------------------------------------------------------------------------------------------------

/// `Item` of the stream that all actor states should implement
pub enum StreamItem<A: Actor + ?Sized> {
    Action(Action<A>),
    Flow(MsgFlow<A>),
}

impl<A: Actor> StreamItem<A> {
    /// If this is an action, handle it and return the flow.
    /// Otherwise just return the flow directly.
    pub async fn handle(self, actor: &mut A) -> EventFlow<A> {
        match self {
            StreamItem::Action(action) => action.handle(actor).await,
            StreamItem::Flow(flow) => flow.into_event_flow(),
        }
    }
}

//--------------------------------------------------------------------------------------------------
//  BasicCtx
//--------------------------------------------------------------------------------------------------

/// This is the default state implementation, that should be fine for 90% of use cases. It offers
/// a lot of functionality, while still being quite fast.
pub struct BasicCtx<A: Actor + ?Sized> {
    address: Address<A>,
    futures: FuturesUnordered<Pin<Box<dyn Future<Output = StreamItem<A>> + Send + Sync + 'static>>>,
    streams:
        stream::SelectAll<Pin<Box<(dyn Stream<Item = StreamItem<A>> + Send + Sync + 'static)>>>,
}


impl<A: Actor + ?Sized> BasicCtx<A> {
    pub fn address(&self) -> &Address<A> {
        &self.address
    }

    pub fn new(address: Address<A>) -> Self {
        Self {
            address,
            futures: FuturesUnordered::new(),
            streams: stream::SelectAll::new(),
        }
    }

    /// Schedule a future that should be ran on this actor. It will be executed on the actor
    /// with priority over incoming messages.
    pub fn schedule<F: 'static + Future<Output = MsgFlow<A>> + Send + Sync + 'static>(
        &mut self,
        future: F,
    ) {
        let future = Box::pin(async move { StreamItem::Flow(future.await) });
        self.futures.push(future)
    }

    /// Schedule a future that should be run on this actor. It will be executed on the actor
    /// with priority over incoming messages.
    ///
    /// After the future completes, the function will be handled.
    pub fn schedule_and<F: Future<Output = P> + Send + Sync + 'static, P: Send + 'static>(
        &mut self,
        future: F,
        function: MsgFn<A, fn(P)>,
    ) where
        A: Sized,
    {
        let future =
            Box::pin(async move { StreamItem::Action(Action::new(function, future.await)) });
        self.futures.push(future)
    }

    /// Add a stream that should be listened to on this actor. Any time a message is received on this stream, it
    /// is handled using the supplied handler function
    pub fn listen<'a, S, P>(&mut self, stream: S, function: MsgFn<A, fn(P)>)
    where
        S: Stream<Item = P> + Send + Sync + 'static,
        P: Send + Sync + 'static,
    {
        let stream = stream.map(move |item| StreamItem::Action(Action::new(function, item)));
        self.streams.push(Box::pin(stream))
    }
}

impl<A: Actor + ?Sized> Stream for BasicCtx<A> {
    type Item = StreamItem<A>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        match self.futures.poll_next_unpin(cx) {
            Poll::Ready(Some(val)) => Poll::Ready(Some(val)),
            Poll::Pending | Poll::Ready(None) => self.streams.poll_next_unpin(cx),
        }
    }
}

impl<A: Actor> Debug for BasicCtx<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultCtx")
            .field("address", &self.address)
            .finish()
    }
}

impl<A: Actor + ?Sized> Unpin for BasicCtx<A> {}

#[derive(Debug)]
pub struct NoCtx<A>(PhantomData<A>);

impl<A: Actor + ?Sized> Stream for NoCtx<A> {
    type Item = StreamItem<A>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }
}

impl<A> NoCtx<A> {
    pub fn new() -> &'static mut Self {
        unsafe { transmute(&mut Self(PhantomData)) }
    }
}

use crate::{actor::Actor, flows::Flow};
use futures::Future;
use std::{any::Any, intrinsics::transmute, pin::Pin};

//--------------------------------------------------------------------------------------------------
//  Action
//--------------------------------------------------------------------------------------------------

/// An action, which can be handled by passing in the actor and the state. The actor can then
/// execute this action within it's event-loop. Actions can be either sync or async, created by
/// `new_sync`, or `new_async`.
#[derive(Debug)]
pub enum Action<A: Actor> {
    Sync(SyncAction<A, Flow<A>>),
    Async(AsyncAction<A, Flow<A>>),
}

impl<A: Actor> Action<A> {
    /// Handle this action
    pub(crate) async fn handle(self, actor: &mut A, state: &mut A::State) -> Flow<A> {
        match self {
            Action::Sync(action) => action.handle(actor, state),
            Action::Async(action) => action.handle(actor, state).await,
        }
    }

    /// Create a new asynchronous action, that can be handled by an [Actor] in it's event-loop.
    pub fn new_async<P, F>(params: P, function: fn(&mut A, &mut A::State, P) -> F) -> Self
    where
        P: 'static + Send,
        F: Future<Output = Flow<A>> + 'static + Send,
    {
        Self::Async(AsyncAction::new(params, function))
    }

    /// Create a new synchronous action, that can be handled by an [Actor] in it's event-loop.
    pub fn new_sync<P>(params: P, function: fn(&mut A, &mut A::State, P) -> Flow<A>) -> Self
    where
        P: 'static + Send,
    {
        Self::Sync(SyncAction::new(params, function))
    }
}

//--------------------------------------------------------------------------------------------------
//  AsyncAction
//--------------------------------------------------------------------------------------------------

/// An asynchronous subtype of [Action]. Users should use [Action] directly.
pub struct AsyncAction<A: Actor, O> {
    handler_fn: fn(
        &mut A,
        &mut A::State,
        Box<dyn Any + Send>,
        usize,
    ) -> Pin<Box<dyn Future<Output = O> + Send + 'static>>,
    actual_fn: usize,
    params: Box<dyn Any + Send>,
}

impl<A: Actor> AsyncAction<A, Flow<A>> {
    /// Create a new async [Action].
    pub(crate) fn new<P: 'static + Send, F: Future<Output = Flow<A>> + Send + 'static>(
        params: P,
        function: fn(&mut A, &mut A::State, P) -> F,
    ) -> Self {
        Self {
            handler_fn: |actor: &mut A,
                         state: &mut A::State,
                         params: Box<dyn Any + Send>,
                         actual_fn: usize| {
                let function: fn(&mut A, &mut A::State, P) -> F = unsafe { transmute(actual_fn) };
                Box::pin(function(actor, state, *params.downcast().unwrap()))
            },
            actual_fn: function as usize,
            params: Box::new(params),
        }
    }

    pub(crate) async fn handle(self, actor: &mut A, state: &mut A::State) -> Flow<A> {
        (self.handler_fn)(actor, state, self.params, self.actual_fn).await
    }
}

impl<A: Actor, O> std::fmt::Debug for AsyncAction<A, O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncAction").finish()
    }
}

//--------------------------------------------------------------------------------------------------
//  SyncAction
//--------------------------------------------------------------------------------------------------

/// An asynchronous subtype of [Action]. Users should use [Action] directly.
pub struct SyncAction<A: Actor, O> {
    handler_fn: fn(&mut A, &mut A::State, Box<dyn Any + Send>, usize) -> O,
    actual_fn: usize,
    params: Box<dyn Any + Send>,
}

impl<A: Actor> SyncAction<A, Flow<A>> {
    /// Create a new sync [Action]
    pub(crate) fn new<P: 'static + Send>(
        params: P,
        function: fn(&mut A, &mut A::State, P) -> Flow<A>,
    ) -> Self {
        Self {
            handler_fn: |actor: &mut A,
                         state: &mut A::State,
                         params: Box<dyn Any + Send>,
                         actual_fn: usize| {
                let function: fn(&mut A, &mut A::State, P) -> Flow<A> =
                    unsafe { transmute(actual_fn) };
                function(actor, state, *params.downcast().unwrap())
            },
            actual_fn: function as usize,
            params: Box::new(params),
        }
    }

    pub(crate) fn handle(self, actor: &mut A, state: &mut A::State) -> Flow<A> {
        (self.handler_fn)(actor, state, self.params, self.actual_fn)
    }
}

impl<A: Actor, O> std::fmt::Debug for SyncAction<A, O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncAction")
            .field("handler_fn", &"func")
            .field("actual_fn", &self.actual_fn)
            .field("params", &self.params)
            .finish()
    }
}

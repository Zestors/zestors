use crate::{
    actor::Actor,
    flows::{EventFlow, MsgFlow, ReqFlow},
    messaging::InternalRequest,
};
use futures::{Future, FutureExt};
use std::{any::Any, intrinsics::transmute, pin::Pin};

//--------------------------------------------------------------------------------------------------
//  helper types
//--------------------------------------------------------------------------------------------------

pub(crate) type MsgFn<'b, A, P> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> MsgFlow<A>;
pub(crate) type AsyncMsgFn<'b, A, P, F> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> F;
pub(crate) type ReqFn<'b, A, P, R> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> ReqFlow<A, R>;
pub(crate) type AsyncReqFn<'b, A, P, F> = fn(&'b mut A, &'b mut <A as Actor>::State, P) -> F;

//--------------------------------------------------------------------------------------------------
//  Action
//--------------------------------------------------------------------------------------------------

/// An action, which can be handled by passing in the actor and the state. The actor can then
/// execute this action within it's event-loop. Actions can be either sync or async, created by
/// `new_sync`, or `new_async`.
#[derive(Debug)]
pub struct Action<A: Actor + ?Sized> {
    inner: InnerAction<A>,
}

/// Just a helper type, so that sync and async action don't have to be public.
pub(crate) enum InnerAction<A: Actor + ?Sized> {
    Sync(SyncAction<A>),
    Async(AsyncAction<A>),
}

impl<A: Actor + ?Sized> std::fmt::Debug for InnerAction<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sync(arg0) => f.debug_tuple("Sync").finish(),
            Self::Async(arg0) => f.debug_tuple("Async").finish(),
        }
    }
}

impl<A: Actor> Action<A> {
    /// Handle this action
    pub(crate) async fn handle(self, actor: &mut A, state: &mut A::State) -> EventFlow<A> {
        match self.inner {
            InnerAction::Sync(action) => action.handle(actor, state),
            InnerAction::Async(action) => action.handle(actor, state).await,
        }
    }

    /// Create a new synchronous action, that can be handled by an [Actor] in it's event-loop.
    pub fn new_sync<'a, P>(params: P, function: MsgFn<'a, A, P>) -> Self
    where
        P: 'static + Send,
    {
        Self {
            inner: InnerAction::Sync(SyncAction::new_msg(params, function)),
        }
    }

    /// Create a new asynchronous action, that can be handled by an [Actor] in it's event-loop.
    pub fn new_async<'a, P, F>(params: P, function: AsyncMsgFn<'a, A, P, F>) -> Self
    where
        P: 'static + Send,
        F: Future<Output = MsgFlow<A>> + 'static + Send,
    {
        Self {
            inner: InnerAction::Async(AsyncAction::new_msg(params, function)),
        }
    }

    /// Attempt to retrieve the parameters from this `Action` through downcasting.
    // This is only valid if this is created through a msg!!!
    pub fn get_params<P: 'static>(self) -> Result<P, Self> {
        match self.inner {
            InnerAction::Sync(action) => match action.params.downcast() {
                Ok(params) => Ok(*params),
                Err(params) => Err(Action {
                    inner: InnerAction::Sync(SyncAction {
                        handler_fn: action.handler_fn,
                        actual_fn: action.actual_fn,
                        params,
                    }),
                }),
            },
            InnerAction::Async(action) => match action.params.downcast() {
                Ok(params) => Ok(*params),
                Err(params) => Err(Action {
                    inner: InnerAction::Async(AsyncAction {
                        handler_fn: action.handler_fn,
                        actual_fn: action.actual_fn,
                        params,
                    }),
                }),
            },
        }
    }

    /// Attempt to retrieve the parameters from this [Action] through downcasting.
    ///
    /// This is only valid if this was created as a request!!
    pub(crate) fn get_params_req<P: 'static, R: 'static>(self) -> Result<P, Self> {
        match self.inner {
            InnerAction::Sync(action) => {
                match action.params.downcast::<(P, InternalRequest<R>)>() {
                    Ok(boxed) => Ok(boxed.0),
                    Err(params) => Err(Action {
                        inner: InnerAction::Sync(SyncAction {
                            handler_fn: action.handler_fn,
                            actual_fn: action.actual_fn,
                            params,
                        }),
                    }),
                }
            }
            InnerAction::Async(action) => match action.params.downcast::<(P, InternalRequest<R>)>()
            {
                Ok(boxed) => Ok(boxed.0),
                Err(params) => Err(Action {
                    inner: InnerAction::Async(AsyncAction {
                        handler_fn: action.handler_fn,
                        actual_fn: action.actual_fn,
                        params,
                    }),
                }),
            },
        }
    }

    pub(crate) fn new_sync_req<P, R>(
        params: P,
        request: InternalRequest<R>,
        function: ReqFn<A, P, R>,
    ) -> Self
    where
        R: 'static + Send,
        P: 'static + Send,
    {
        Self {
            inner: InnerAction::Sync(SyncAction::new_req(params, request, function)),
        }
    }

    pub(crate) fn new_async_req<P, R, F>(
        params: P,
        request: InternalRequest<R>,
        function: AsyncReqFn<A, P, F>,
    ) -> Self
    where
        R: 'static + Send,
        P: 'static + Send,
        F: Future<Output = ReqFlow<A, R>> + Send + 'static,
    {
        Self {
            inner: InnerAction::Async(AsyncAction::new_req(params, request, function)),
        }
    }
}

//--------------------------------------------------------------------------------------------------
//  AsyncAction
//--------------------------------------------------------------------------------------------------

/// An asynchronous subtype of [Action]. Users should use [Action] directly.
pub(crate) struct AsyncAction<A: Actor + ?Sized> {
    handler_fn: fn(
        &mut A,
        &mut A::State,
        Box<dyn Any + Send>,
        usize,
    ) -> Pin<Box<dyn Future<Output = EventFlow<A>> + Send + 'static>>,
    actual_fn: usize,
    params: Box<dyn Any + Send>,
}

impl<A: Actor> AsyncAction<A> {
    /// Create a new async [Action].
    pub(crate) fn new_msg<P, F>(params: P, function: AsyncMsgFn<A, P, F>) -> Self
    where
        P: 'static + Send,
        F: Future<Output = MsgFlow<A>> + Send + 'static,
    {
        Self {
            handler_fn: |actor: &mut A,
                         state: &mut A::State,
                         params: Box<dyn Any + Send>,
                         actual_fn: usize| {
                let function: AsyncMsgFn<A, P, F> = unsafe { transmute(actual_fn) };

                Box::pin(
                    function(actor, state, *params.downcast().unwrap()).map(|f| f.into_internal()),
                )
            },
            actual_fn: function as usize,
            params: Box::new(params),
        }
    }

    /// Create a new async [Action].
    pub(crate) fn new_req<P, R, F>(
        params: P,
        request: InternalRequest<R>,
        function: AsyncReqFn<A, P, F>,
    ) -> Self
    where
        P: 'static + Send,
        R: Send + 'static,
        F: Future<Output = ReqFlow<A, R>> + Send + 'static,
    {
        Self {
            handler_fn: |actor: &mut A,
                         state: &mut A::State,
                         params: Box<dyn Any + Send>,
                         actual_fn: usize| {
                let function: AsyncReqFn<A, P, F> = unsafe { transmute(actual_fn) };

                let (params, request): (P, InternalRequest<R>) = *params.downcast().unwrap();

                Box::pin(function(actor, state, params).map(|f| {
                    let (flow, reply) = f.take_reply();

                    if let Some(reply) = reply {
                        request.reply(reply);
                    }

                    flow
                }))
            },
            actual_fn: function as usize,
            params: Box::new((params, request)),
        }
    }

    pub(crate) async fn handle(self, actor: &mut A, state: &mut A::State) -> EventFlow<A> {
        (self.handler_fn)(actor, state, self.params, self.actual_fn).await
    }
}

impl<A: Actor> std::fmt::Debug for AsyncAction<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncAction").finish()
    }
}

//--------------------------------------------------------------------------------------------------
//  SyncAction
//--------------------------------------------------------------------------------------------------

/// An asynchronous subtype of [Action]. Users should use [Action] directly.
pub(crate) struct SyncAction<A: Actor + ?Sized> {
    handler_fn: fn(&mut A, &mut A::State, Box<dyn Any + Send>, usize) -> EventFlow<A>,
    actual_fn: usize,
    params: Box<dyn Any + Send>,
}

impl<A: Actor> SyncAction<A> {
    /// Create a new sync [Action]
    pub(crate) fn new_msg<'a, P: 'static + Send>(params: P, function: MsgFn<A, P>) -> Self {
        Self {
            handler_fn: |actor: &mut A,
                         state: &mut A::State,
                         params: Box<dyn Any + Send>,
                         actual_fn: usize| {
                let function: MsgFn<A, P> = unsafe { transmute(actual_fn) };
                function(actor, state, *params.downcast().unwrap()).into_internal()
            },
            actual_fn: function as usize,
            params: Box::new(params),
        }
    }

    /// Create a new sync [Action]
    pub(crate) fn new_req<P: 'static + Send, R: Send + 'static>(
        params: P,
        request: InternalRequest<R>,
        function: ReqFn<A, P, R>,
    ) -> Self {
        Self {
            handler_fn: |actor: &mut A,
                         state: &mut A::State,
                         params: Box<dyn Any + Send>,
                         actual_fn: usize| {
                let function: ReqFn<A, P, R> = unsafe { transmute(actual_fn) };

                let (params, request): (P, InternalRequest<R>) = *params.downcast().unwrap();

                let (flow, reply) = function(actor, state, params).take_reply();

                if let Some(reply) = reply {
                    request.reply(reply)
                }

                flow
            },
            actual_fn: function as usize,
            params: Box::new((params, request)),
        }
    }

    pub(crate) fn handle(self, actor: &mut A, state: &mut A::State) -> EventFlow<A> {
        (self.handler_fn)(actor, state, self.params, self.actual_fn)
    }
}

impl<A: Actor> std::fmt::Debug for SyncAction<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncAction")
            .field("handler_fn", &"func")
            .field("actual_fn", &self.actual_fn)
            .field("params", &self.params)
            .finish()
    }
}

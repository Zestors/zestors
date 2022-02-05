use std::{any::Any, intrinsics::transmute, pin::Pin};

use futures::Future;

use crate::{
    actor::Actor,
    flow::{Flow, MsgFlow},
};

//-------------------------------------
// Before
//-------------------------------------

pub struct Before<A: Actor>(AsyncAction<A, Flow<A>>);

unsafe impl<A: Actor> Send for Before<A> {}

impl<A: Actor> Before<A> {
    pub fn new<P: 'static + Send, F: Future<Output = Flow<A>> + Send + 'static>(
        params: P,
        function: fn(&mut A, &mut A::State, P) -> F,
    ) -> Self {
        Self(AsyncAction::<A, Flow<A>>::new(params, function))
    }

    pub(crate) async fn handle(self, actor: &mut A, state: &mut A::State) -> Flow<A> {
        self.0.handle(actor, state).await
    }
}

//-------------------------------------
// After
//-------------------------------------

pub struct After<A: Actor>(AsyncAction<A, MsgFlow<A>>);

unsafe impl<A: Actor> Send for After<A> {}

impl<A: Actor> After<A> {
    pub fn new<P: 'static + Send, F: Future<Output = MsgFlow<A>> + Send + 'static>(
        params: P,
        function: fn(&mut A, &mut A::State, P) -> F,
    ) -> Self {
        Self(AsyncAction::<A, MsgFlow<A>>::new(params, function))
    }

    pub(crate) async fn handle(self, actor: &mut A, state: &mut A::State) -> MsgFlow<A> {
        self.0.handle(actor, state).await
    }
}

//-------------------------------------
// Action
//-------------------------------------

pub enum Action<A: Actor, O> {
    Sync(SyncAction<A, O>),
    Async(AsyncAction<A, O>)
}

//-------------------------------------
// AsyncAction
//-------------------------------------

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

unsafe impl<A: Actor, O> Send for AsyncAction<A, O> {}

impl<A: Actor> AsyncAction<A, MsgFlow<A>> {
    pub fn new<P: 'static + Send, F: Future<Output = MsgFlow<A>> + Send + 'static>(
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

    pub(crate) async fn handle(self, actor: &mut A, state: &mut A::State) -> MsgFlow<A> {
        (self.handler_fn)(actor, state, self.params, self.actual_fn).await
    }
}

impl<A: Actor> AsyncAction<A, Flow<A>> {
    pub fn new<P: 'static + Send, F: Future<Output = Flow<A>> + Send + 'static>(
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

//-------------------------------------
// SyncAction
//-------------------------------------

pub struct SyncAction<A: Actor, O> {
    handler_fn: fn(
        &mut A,
        &mut A::State,
        Box<dyn Any + Send>,
        usize,
    ) -> O,
    actual_fn: usize,
    params: Box<dyn Any + Send>,
}

unsafe impl<A: Actor, O> Send for SyncAction<A, O> {}

impl<A: Actor> SyncAction<A, MsgFlow<A>> {
    pub fn new<P: 'static + Send>(
        params: P,
        function: fn(&mut A, &mut A::State, P) -> MsgFlow<A>,
    ) -> Self {
        Self {
            handler_fn: |actor: &mut A,
                         state: &mut A::State,
                         params: Box<dyn Any + Send>,
                         actual_fn: usize| {
                let function: fn(&mut A, &mut A::State, P) -> MsgFlow<A> = unsafe { transmute(actual_fn) };
                function(actor, state, *params.downcast().unwrap())
            },
            actual_fn: function as usize,
            params: Box::new(params),
        }
    }

    pub(crate) fn handle(self, actor: &mut A, state: &mut A::State) -> MsgFlow<A> {
        (self.handler_fn)(actor, state, self.params, self.actual_fn)
    }
}

impl<A: Actor> SyncAction<A, Flow<A>> {
    pub fn new<P: 'static + Send>(
        params: P,
        function: fn(&mut A, &mut A::State, P) -> MsgFlow<A>,
    ) -> Self {
        Self {
            handler_fn: |actor: &mut A,
                         state: &mut A::State,
                         params: Box<dyn Any + Send>,
                         actual_fn: usize| {
                let function: fn(&mut A, &mut A::State, P) -> Flow<A> = unsafe { transmute(actual_fn) };
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
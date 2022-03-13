use futures::Future;
use std::{
    any::Any,
    fmt::Debug,
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use crate::{
    action::Action,
    actor::{Actor, ProcessId},
    distributed::pid::ProcessRef,
    errors::{DidntArrive, ReqRecvError},
    flows::{EventFlow, MsgFlow, ReqFlow},
    function::{MsgFn, ReqFn},
    inbox::ActionSender,
    messaging::{Msg, Reply, Req, Request},
};

//--------------------------------------------------------------------------------------------------
//  Address definition
//--------------------------------------------------------------------------------------------------

pub struct Address<A: ?Sized> {
    a: PhantomData<A>,
    sender: ActionSender<A>,
    process_id: ProcessId,
}

unsafe impl<A> Sync for Address<A> {}

impl<'params, A: Actor> Address<A> {
    pub(crate) fn new(sender: ActionSender<A>, process_id: ProcessId) -> Self {
        Self {
            sender,
            a: PhantomData,
            process_id,
        }
    }

    pub fn process_id(&self) -> ProcessId {
        self.process_id
    }

    pub fn msg<'a, P>(&'a self, function: MsgFn<A, fn(P)>, params: P) -> Result<(), DidntArrive<P>>
    where
        P: Send + 'static,
    {
        Msg::new(&self.sender, Action::new(function, params)).send()
    }

    pub unsafe fn req_ref<'a, P, R>(
        &self,
        function: ReqFn<A, fn(P) -> R>,
        params: P,
    ) -> Result<Reply<R>, DidntArrive<P>>
    where
        P: Send + 'a,
        R: Send + 'a,
    {
        let (request, reply) = Request::new();
        Req::new(
            &self.sender,
            Action::new_req(function, params, request),
            reply,
        )
        .send_ref()
    }

    pub async fn req_recv<'a, P, R>(
        &self,
        function: ReqFn<A, fn(P) -> R>,
        params: P,
    ) -> Result<R, ReqRecvError<P>>
    where
        P: Send + 'a,
        R: Send + 'a,
    {
        let (request, reply) = Request::new();
        unsafe {
            Ok(Req::new(
                &self.sender,
                Action::new_req(function, params, request),
                reply,
            )
            .send_ref()?
            .await?)
        }
    }

    pub(crate) unsafe fn send_raw<P: Send + 'static>(
        &self,
        action: Action<A>,
    ) -> Result<(), DidntArrive<P>> {
        Msg::new(&self.sender, action).send()
    }

    pub fn req<P, R>(
        &self,
        function: ReqFn<A, fn(P) -> R>,
        params: P,
    ) -> Result<Reply<R>, DidntArrive<P>>
    where
        P: Send + 'static,
        R: Send + 'static,
    {
        let (request, reply) = Request::new();
        Req::new(
            &self.sender,
            Action::new_req(function, params, request),
            reply,
        )
        .send()
    }
}

//------------------------------------------------------------------------------------------------
//  AnyAddress
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct AnyAddress(Box<dyn Any + Send + Sync>);

impl AnyAddress {
    pub fn new<A: Actor>(address: Address<A>) -> Self {
        Self(Box::new(address))
    }

    pub fn downcast<A: 'static + Actor>(self) -> Result<Address<A>, Self> {
        match self.0.downcast() {
            Ok(pid) => Ok(*pid),
            Err(boxed) => Err(Self(boxed)),
        }
    }

    pub fn downcast_ref<A: Actor>(&self) -> Option<&Address<A>> {
        self.0.downcast_ref()
    }
}

//--------------------------------------------------------------------------------------------------
//  Addressable + RawAddress traits
//--------------------------------------------------------------------------------------------------

/// A trait that allows a custom address to be callable with `address.msg()`, or `address.req_async()`
/// etc. You will probably also want to implement `From<Address<Actor>>` for this custom address,
/// to allow this address to be used as the associated type [Actor::Address].
///
/// This trait has no methods that need to be implemented, but it is necessary to implement
/// [RawAddress] in order to use this trait.
///
/// This can be derived using [crate::derive::Addressable].
pub trait Addressable<A: Actor> {
    fn addr(&self) -> &Address<A>;

    /// Whether this process is still alive.
    fn is_alive(&self) -> bool {
        self.addr().sender.is_alive()
    }

    fn process_id(&self) -> ProcessId {
        self.addr().process_id()
    }
}

//--------------------------------------------------------------------------------------------------
//  Implement traits for Address
//--------------------------------------------------------------------------------------------------

impl<A: Actor> Addressable<A> for Address<A> {
    fn addr(&self) -> &Address<A> {
        self
    }
}

impl<A: Actor> Clone for Address<A> {
    fn clone(&self) -> Self {
        Self {
            a: PhantomData,
            sender: self.sender.clone(),
            process_id: self.process_id.clone(),
        }
    }
}

impl<A: Actor> std::fmt::Debug for Address<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Address")
            .field("name", &<A as Actor>::debug())
            .field("id", &self.process_id)
            .finish()
    }
}

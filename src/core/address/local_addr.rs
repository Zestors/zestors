use crate::core::*;
use futures::{Future, Stream};
use std::{
    any::Any, convert::Infallible, error::Error, marker::PhantomData, pin::Pin, sync::Arc,
    task::Poll,
};

/// An address for processes within the same system.
///
/// Addresses can be freely cloned, and can be used to send messages to the actor.
/// Messages can be sent by either `call`ing a process with a function, or by
/// `send`ing an `Action`.
pub struct LocalAddr<A> {
    sender: async_channel::Sender<InternalMsg<A>>,
    shared: Arc<SharedProcessData>,
}

impl<A> LocalAddr<A> {
    pub(crate) fn new(
        sender: async_channel::Sender<InternalMsg<A>>,
        shared: Arc<SharedProcessData>,
    ) -> Self {
        Self { sender, shared }
    }

    /// Send an action to this address
    pub fn send(&self, action: Action<A>) -> Result<(), AddrSndError<Action<A>>> {
        self.sender
            .try_send(InternalMsg::Action(action))
            .map_err(|e| {
                let InternalMsg::Action(action) = e.into_inner();
                AddrSndError(action)
            })
    }

    /// Get the amount of messages currently in the inbox.
    pub fn msg_count(&self) -> usize {
        self.sender.len()
    }

    /// Whether this actor still accepts new messages.
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }

    /// Whether the process has exited.
    pub fn has_exited(&self) -> bool {
        self.shared.has_exited()
    }

    /// Get the unique process id of this process
    pub fn process_id(&self) -> ProcessId {
        self.shared.process_id()
    }

    /// Get the amount of addresses of this actor.
    pub fn addr_count(&self) -> usize {
        self.sender.sender_count()
    }

    /// Call this address with a function.
    pub fn call<M, R>(
        &self,
        function: HandlerFn<A, M, R>,
        msg: LocalMsg<M>,
    ) -> Result<R, AddrSndError<M>>
    where
        M: Send + 'static,
        R: RcvPart,
    {
        let (action, receiver) = Action::new_split(function, msg.0);
        if let Err(AddrSndError(action)) = self.send(action) {
            let (msg, _snd) = match action.downcast::<M, R>() {
                Ok(msg) => msg,
                Err(_) => unreachable!(),
            };
            return Err(AddrSndError(msg));
        }
        Ok(receiver)
    }
}

impl<A> std::fmt::Debug for LocalAddr<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalAddr")
            .field("sender", &self.sender)
            .field("shared", &self.shared)
            .finish()
    }
}

impl<A> Clone for LocalAddr<A> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            shared: self.shared.clone(),
        }
    }
}

//------------------------------------------------------------------------------------------------
//  Local
//------------------------------------------------------------------------------------------------

/// This address is a local address.
#[derive(Clone, Debug)]
pub struct Local;

impl AddrType for Local {
    type Action<A> = Action<A>;
    type Addr<A: 'static> = LocalAddr<A>;
    type CallResult<M, R: RcvPart> = Result<R, AddrSndError<M>>;
    type SendResult<A> = Result<(), AddrSndError<Action<A>>>;
    type Msg<MT> = LocalMsg<MT>;
}

/// The ParamType `P` of a Local address is always just `P` whenver `P` is Send + 'static
impl<'a, P> ParamType<'a, Local> for P
where
    P: Send + 'static,
{
    type Param = P;
}

//------------------------------------------------------------------------------------------------
//  LocalAddr
//------------------------------------------------------------------------------------------------
#[derive(Debug)]
pub enum SndRcvError<M> {
    SndFailure(M),
    RcvFailure,
}

//------------------------------------------------------------------------------------------------
//  LocalMsg
//------------------------------------------------------------------------------------------------

pub struct LocalParam<P>(P);
pub struct LocalMsg<M>(M);

impl<M> From<M> for LocalMsg<M> {
    fn from(m: M) -> Self {
        Self(m)
    }
}

//------------------------------------------------------------------------------------------------
//  Address impl
//------------------------------------------------------------------------------------------------

impl<A: 'static> Address for LocalAddr<A> {
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_mut_any(&mut self) -> &mut dyn Any {
        self
    }

    type AddrType = Local;
}

//------------------------------------------------------------------------------------------------
//  Addressable impl
//------------------------------------------------------------------------------------------------

impl<A: 'static> Addressable<A> for LocalAddr<A> {
    fn from_addr(addr: LocalAddr<A>) -> Self {
        addr
    }

    fn inner(&self) -> &LocalAddr<A> {
        self
    }

    fn call<P, M, R>(
        &self,
        function: HandlerFn<A, M, R>,
        params: P,
    ) -> <Self::AddrType as AddrType>::CallResult<M, R>
    where
        P: Into<LocalMsg<M>>,
        R: RcvPart,
        M: Send + 'static,
    {
        self.call(function, params.into())
    }

    fn send<T>(&self, action: T) -> <Self::AddrType as AddrType>::SendResult<A>
    where
        T: Into<<Self::AddrType as AddrType>::Action<A>>,
    {
        self.send(action.into())
    }
}

//------------------------------------------------------------------------------------------------
//  LocalAddrError
//------------------------------------------------------------------------------------------------

/// An error returned when trying to `call` or `send` to a local address. This process is no longer
/// alive, and can no longer accept messges.
#[derive(Debug)]
pub struct AddrSndError<T>(pub T);

impl<A> From<async_channel::TrySendError<InternalMsg<A>>> for SndError<Action<A>> {
    fn from(e: async_channel::TrySendError<InternalMsg<A>>) -> Self {
        let InternalMsg::Action(action) = e.into_inner();
        Self(action)
    }
}

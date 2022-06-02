use crate::core::*;
use std::{any::Any, sync::Arc};

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

    /// Wait for the process to exit.
    ///
    /// If the process has already exited, this returns immeadeately.
    pub async fn await_exit(&self) {
        self.shared.await_exit().await
    }

    /// Send a message to this address.
    pub fn send<M, R>(
        &self,
        function: HandlerFn<A, M, R>,
        msg: impl Into<LocalMsg<M>>,
    ) -> Result<R, AddrSndError<M>>
    where
        M: Send + 'static,
        R: RcvPart,
    {
        let (action, receiver) = Action::new_split(function, msg.into().0);
        if let Err(AddrSndError(action)) = self.send_action(action) {
            let (msg, _snd) = match action.downcast::<M, R>() {
                Ok(msg) => msg,
                Err(_) => unreachable!(),
            };
            return Err(AddrSndError(msg));
        }
        Ok(receiver)
    }

    /// Send an action to this address
    pub fn send_action(&self, action: impl Into<Action<A>>) -> Result<(), AddrSndError<Action<A>>> {
        self.sender
            .try_send(InternalMsg::Action(action.into()))
            .map_err(|e| {
                let InternalMsg::Action(action) = e.into_inner();
                AddrSndError(action)
            })
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

    // fn msg_count(&self) -> usize {
    //     self.msg_count()
    // }

    // fn is_closed(&self) -> bool {
    //     self.is_closed()
    // }

    // fn has_exited(&self) -> bool {
    //     self.has_exited()
    // }

    // fn process_id(&self) -> ProcessId {
    //     self.process_id()
    // }

    // fn addr_count(&self) -> usize {
    //     self.
    // }

    // fn await_exit(&self) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + Send + 'static>> {
    //     todo!()
    // }
}

//------------------------------------------------------------------------------------------------
//  Addressable impl
//------------------------------------------------------------------------------------------------

impl<A: 'static> Addressable<A> for LocalAddr<A> {
    fn from_addr(addr: LocalAddr<A>) -> Self {
        addr
    }

    fn inner(addr: &Self) -> &LocalAddr<A> {
        addr
    }

    fn send<P, M, R>(
        addr: &Self,
        function: HandlerFn<A, M, R>,
        params: P,
    ) -> <Self::AddrType as AddrType>::CallResult<M, R>
    where
        P: Into<LocalMsg<M>>,
        R: RcvPart,
        M: Send + 'static,
    {
        addr.send(function, params.into())
    }

    fn send_action<T>(addr: &Self, action: T) -> <Self::AddrType as AddrType>::SendResult<A>
    where
        T: Into<<Self::AddrType as AddrType>::Action<A>>,
    {
        addr.send_action(action.into())
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

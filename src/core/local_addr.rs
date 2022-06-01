use crate::core::*;
use futures::{Future, Stream};
use std::{any::Any, convert::Infallible, error::Error, marker::PhantomData, pin::Pin, task::Poll};

//------------------------------------------------------------------------------------------------
//  Local
//------------------------------------------------------------------------------------------------

/// This address is a local address.
#[derive(Clone, Debug)]
pub struct Local;

impl AddrType for Local {
    type Action<A> = Action<A>;
    type Addr<A: 'static> = LocalAddr<A>;
    type CallResult<M, R: RcvPart> = Result<R, LocalAddrError<M>>;
    type SendResult<A> = Result<(), LocalAddrError<Action<A>>>;
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

pub trait IntoRcv {
    type Output;
    fn into_rcv(self) -> Pin<Box<dyn Future<Output = Self::Output> + Send + 'static>>;
}

impl<M: Send + 'static, R: Send + 'static> IntoRcv for Result<Rcv<R>, LocalAddrError<M>> {
    type Output = Result<R, SndRcvError<M>>;
    fn into_rcv(self) -> Pin<Box<dyn Future<Output = Self::Output> + Send + 'static>> {
        Box::pin(async move {
            match self {
                Ok(rcv) => match rcv.await {
                    Ok(r) => Ok(r),
                    Err(_e) => Err(SndRcvError::RcvFailure),
                },
                Err(e) => Err(SndRcvError::SndFailure(e.0)),
            }
        })
    }
}

/// An address for processes located within the same system.
pub struct LocalAddr<A> {
    sender: async_channel::Sender<InternalMsg<A>>,
    process_id: ProcessId,
}

impl<A> LocalAddr<A> {
    pub(crate) fn new(
        sender: async_channel::Sender<InternalMsg<A>>,
        process_id: ProcessId,
    ) -> Self {
        Self { sender, process_id }
    }

    pub(crate) fn _send(&self, action: Action<A>) -> Result<(), LocalAddrError<Action<A>>> {
        self.sender
            .try_send(InternalMsg::Action(action))
            .map_err(|e| {
                let InternalMsg::Action(action) = e.into_inner();
                LocalAddrError(action)
            })
    }

    pub(crate) fn _call_addr<M, R>(
        &self,
        function: HandlerFn<A, Snd<M>, R>,
        msg: LocalMsg<M>,
    ) -> Result<R, LocalAddrError<M>>
    where
        M: Send + 'static,
        R: RcvPart,
    {
        let (action, receiver) = Action::new_split(function, msg.0);
        if let Err(LocalAddrError(action)) = self._send(action) {
            let (msg, _snd) = match action.downcast::<M, R>() {
                Ok(msg) => msg,
                Err(_) => unreachable!(),
            };
            return Err(LocalAddrError(msg));
        }
        Ok(receiver)
    }
}

impl<A> std::fmt::Debug for LocalAddr<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalAddr")
            .field("sender", &self.sender)
            .field("process_id", &self.process_id)
            .finish()
    }
}

impl<A> Clone for LocalAddr<A> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            process_id: self.process_id.clone(),
        }
    }
}

unsafe impl<A> Send for LocalAddr<A> {}
unsafe impl<A> Sync for LocalAddr<A> {}

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

    fn as_addr(&self) -> &LocalAddr<A> {
        self
    }

    fn call<P, M, R>(
        &self,
        function: HandlerFn<A, Snd<M>, R>,
        params: P,
    ) -> <Self::AddrType as AddrType>::CallResult<M, R>
    where
        P: Into<LocalMsg<M>>,
        R: RcvPart,
        M: Send + 'static,
    {
        self.as_addr()._call_addr(function, params.into())
    }

    fn send<T>(&self, action: T) -> <Self::AddrType as AddrType>::SendResult<A>
    where
        T: Into<<Self::AddrType as AddrType>::Action<A>>,
    {
        self.as_addr()._send(action.into())
    }
}

//------------------------------------------------------------------------------------------------
//  LocalAddrError
//------------------------------------------------------------------------------------------------

/// An error returned when trying to `call` or `send` to a local address. This process is no longer
/// alive.
#[derive(Debug)]
pub struct LocalAddrError<T>(T);

impl<A> From<async_channel::TrySendError<InternalMsg<A>>> for SendError<Action<A>> {
    fn from(e: async_channel::TrySendError<InternalMsg<A>>) -> Self {
        let InternalMsg::Action(action) = e.into_inner();
        Self(action)
    }
}

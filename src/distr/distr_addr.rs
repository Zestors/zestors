use std::{any::Any, marker::PhantomData};

use crate::core::*;
use crate::distr::*;

//------------------------------------------------------------------------------------------------
//  Distr
//------------------------------------------------------------------------------------------------

/// This address is a remote address.
#[derive(Clone, Debug)]
pub struct Distr;

//------------------------------------------------------------------------------------------------
//  AddrType
//------------------------------------------------------------------------------------------------

impl AddrType for Distr {
    type Action<A> = RemoteAction<A>;
    type Addr<A: 'static> = DistrAddr<A>;
    type CallResult<M, R: RcvPart> = Result<R, LocalAddrError<M>>;
    type SendResult<A> = Result<(), LocalAddrError<Action<A>>>;
    type Param<'a, P: 'a> = DistrParam<'a, P> where
    P: ParamType<'a, Self>;
    type Msg<M> = RemoteMsg<M>;
}


//------------------------------------------------------------------------------------------------
//  DistrAddr
//------------------------------------------------------------------------------------------------

/// An address for processes located on another system.
pub struct DistrAddr<A> {
    sender: async_channel::Sender<Vec<u8>>,
    process_id: ProcessId,
    phantom_data: PhantomData<A>,
}

unsafe impl<A> Send for DistrAddr<A> {}

impl<A> DistrAddr<A> {
    fn _send(&self, action: RemoteAction<A>) -> Result<(), LocalAddrError<Action<A>>> {
        todo!()
    }

    fn _call_addr<M, R>(
        &self,
        function: HandlerFn<A, Snd<M>, R>,
        msg: RemoteMsg<M>,
    ) -> Result<R, LocalAddrError<M>>
    where
        M: Send + 'static,
        R: RcvPart,
    {
        todo!()
    }
}



//------------------------------------------------------------------------------------------------
//  Address impl
//------------------------------------------------------------------------------------------------

impl<A: 'static> Address for DistrAddr<A> {
    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_mut_any(&mut self) -> &mut dyn Any {
        self
    }

    type AddrType = Distr;
}

//------------------------------------------------------------------------------------------------
//  Addressable impl
//------------------------------------------------------------------------------------------------

impl<A: 'static> Addressable<A> for DistrAddr<A> {
    fn from_addr(addr: <Self::AddrType as AddrType>::Addr<A>) -> Self {
        addr
    }
    fn as_addr(&self) -> &<Self::AddrType as AddrType>::Addr<A> {
        self
    }

    fn call<P, M, R>(
        &self,
        function: HandlerFn<A, Snd<M>, R>,
        params: P,
    ) -> <Self::AddrType as AddrType>::CallResult<M, R>
    where
        P: Into<RemoteMsg<M>>,
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

impl<A> std::fmt::Debug for DistrAddr<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteAddr")
            .field("sender", &self.sender)
            .field("process_id", &self.process_id)
            .field("phantom_data", &self.phantom_data)
            .finish()
    }
}

impl<A> Clone for DistrAddr<A> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            process_id: self.process_id.clone(),
            phantom_data: self.phantom_data.clone(),
        }
    }
}

//------------------------------------------------------------------------------------------------
//  DistrParam
//------------------------------------------------------------------------------------------------

pub enum DistrParam<'a, P: ParamType<'a, Distr>> {
    Msg(P::Param),
    Rcv(P::Param),
    Snd(P::Param),
}

//------------------------------------------------------------------------------------------------
//  RemoteMsg
//------------------------------------------------------------------------------------------------

pub struct RemoteMsg<M> {
    phantom_data: PhantomData<*const M>,
    serialized: Vec<u8>,
}

impl<'a, P: 'a> From<&'a P> for RemoteMsg<P>
where
    P: DistrParamType<'a> + ParamType<'a, Distr, Param = &'a P>,
{
    fn from(p1: &'a P) -> Self {
        let mut buf = Vec::new();

        // bincode::serialize_into(writer, value);
        // bincode::deserialize_from(writer, value);

        <P as DistrParamType>::serialize_into(p1, &mut buf);

        Self {
            phantom_data: PhantomData,
            serialized: buf,
        }
    }
}

impl<'a, P> From<P> for RemoteMsg<P>
where
    P: DistrParamType<'a> + ParamType<'a, Distr, Param = P>,
{
    fn from(msg: P) -> Self {
        let mut buf = Vec::new();
        <P as DistrParamType>::serialize_into(msg, &mut buf);

        Self {
            phantom_data: PhantomData,
            serialized: buf,
        }
    }
}

impl<'a, P1, P2> From<(Param<'a, P1, Distr>, Param<'a, P2, Distr>)> for RemoteMsg<(P1, P2)>
where
    P1: DistrParamType<'a>,
    P2: DistrParamType<'a>,
{
    fn from(
        (p1, p2): (
            <P1 as ParamType<'a, Distr>>::Param,
            <P2 as ParamType<'a, Distr>>::Param,
        ),
    ) -> Self {
        let mut buf = Vec::new();
        <P1 as DistrParamType>::serialize_into(p1, &mut buf);
        <P2 as DistrParamType>::serialize_into(p2, &mut buf);

        Self {
            phantom_data: PhantomData,
            serialized: buf,
        }
    }
}
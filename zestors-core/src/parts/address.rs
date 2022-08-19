use crate::*;
use event_listener::EventListener;
use futures::{Future, FutureExt};
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

pub struct Address<T: AddressKind> {
    address: T::Address,
}

impl<T: AddressKind> Address<T> {
    fn close(&self) -> bool {
        self.address.close()
    }

    pub fn try_send<M>(&self, msg: M) -> Result<Returns<M>, TrySendError<M>>
    where
        M: Message,
        T: Accepts<M>,
    {
        <T as Accepts<M>>::try_send(&self.address, msg)
    }
}

pub struct DynAddress<D> {
    address: tiny_actor::Address<dyn BoxChannel>,
    phantom_data: PhantomData<D>,
}

pub struct StaticAddress<P> {
    address: tiny_actor::Address<Channel<P>>,
}
impl<D> tiny_actor::DynChannel for DynAddress<D> {
    fn close(&self) -> bool {
        self.address.channel_ref().close()
    }
    fn halt_some(&self, n: u32) {
        self.address.channel_ref().halt_some(n)
    }
    fn inbox_count(&self) -> usize {
        self.address.channel_ref().inbox_count()
    }
    fn msg_count(&self) -> usize {
        self.address.channel_ref().msg_count()
    }
    fn address_count(&self) -> usize {
        self.address.channel_ref().address_count()
    }
    fn is_closed(&self) -> bool {
        self.address.channel_ref().is_closed()
    }
    fn capacity(&self) -> &Capacity {
        self.address.channel_ref().capacity()
    }
    fn has_exited(&self) -> bool {
        self.address.channel_ref().has_exited()
    }
    fn add_address(&self) -> usize {
        self.address.channel_ref().add_address()
    }
    fn remove_address(&self) -> usize {
        self.address.channel_ref().remove_address()
    }
    fn get_exit_listener(&self) -> EventListener {
        self.address.channel_ref().get_exit_listener()
    }
    fn actor_id(&self) -> u64 {
        self.address.channel_ref().actor_id()
    }
    fn is_bounded(&self) -> bool {
        self.address.channel_ref().is_bounded()
    }
}

impl<P> tiny_actor::DynChannel for StaticAddress<P> {
    fn close(&self) -> bool {
        self.address.channel_ref().close()
    }
    fn halt_some(&self, n: u32) {
        self.address.channel_ref().halt_some(n)
    }
    fn inbox_count(&self) -> usize {
        self.address.channel_ref().inbox_count()
    }
    fn msg_count(&self) -> usize {
        self.address.channel_ref().msg_count()
    }
    fn address_count(&self) -> usize {
        self.address.channel_ref().address_count()
    }
    fn is_closed(&self) -> bool {
        self.address.channel_ref().is_closed()
    }
    fn capacity(&self) -> &Capacity {
        self.address.channel_ref().capacity()
    }
    fn has_exited(&self) -> bool {
        self.address.channel_ref().has_exited()
    }
    fn add_address(&self) -> usize {
        self.address.channel_ref().add_address()
    }
    fn remove_address(&self) -> usize {
        self.address.channel_ref().remove_address()
    }
    fn get_exit_listener(&self) -> EventListener {
        self.address.channel_ref().get_exit_listener()
    }
    fn actor_id(&self) -> u64 {
        self.address.channel_ref().actor_id()
    }
    fn is_bounded(&self) -> bool {
        self.address.channel_ref().is_bounded()
    }
}

impl<P> StaticAddress<P> {
    pub fn from_inner(address: tiny_actor::Address<Channel<P>>) -> Self {
        Self { address }
    }

    pub fn try_send<M>(&self, msg: M) -> Result<Returns<M>, TrySendError<M>>
    where
        P: ProtocolMessage<M>,
        M: Message,
    {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);

        match self.address.try_send(P::from_sends(sends)) {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendError::Closed(prot) => {
                    Err(TrySendError::Closed(prot.unwrap_into_msg(returns)))
                }
                TrySendError::Full(prot) => Err(TrySendError::Full(prot.unwrap_into_msg(returns))),
            },
        }
    }

    pub fn send_now<M>(&self, msg: M) -> Result<<M::Type as MsgType<M>>::Returns, TrySendError<M>>
    where
        P: ProtocolMessage<M>,
        M: Message,
    {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);

        match self.address.send_now(P::from_sends(sends)) {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendError::Closed(prot) => {
                    Err(TrySendError::Closed(prot.unwrap_into_msg(returns)))
                }
                TrySendError::Full(prot) => Err(TrySendError::Full(prot.unwrap_into_msg(returns))),
            },
        }
    }

    pub fn send_blocking<M>(&self, msg: M) -> Result<<M::Type as MsgType<M>>::Returns, SendError<M>>
    where
        P: ProtocolMessage<M>,
        M: Message,
    {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);

        match self.address.send_blocking(P::from_sends(sends)) {
            Ok(()) => Ok(returns),
            Err(SendError(prot)) => Err(SendError(prot.unwrap_into_msg(returns))),
        }
    }

    pub fn send<M>(&self, msg: M) -> Snd<'_, M, P>
    where
        P: ProtocolMessage<M>,
        M: Message,
    {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);
        Snd::new(self.address.send(P::from_sends(sends)), returns)
    }

    gen::send_methods!();

    pub fn into_dyn<D>(self) -> DynAddress<D>
    where
        P: Protocol + IntoDyn<D>,
    {
        DynAddress {
            address: self.address.transform_channel(|c| c),
            phantom_data: PhantomData,
        }
    }
}

impl<D> DynAddress<D> {
    fn transform_unchecked<T>(self) -> DynAddress<T> {
        DynAddress::from_inner(self.address)
    }

    /// Transform the `Address<C>` into an `Address<T>`, as long as it accepts
    /// a subset of the messages.
    ///
    /// Some valid transformations are:
    /// * `Address<dyn AcceptsTwo<u32, u64>>` -> `Address<dyn AcceptsTwo<u64, u32>>`.
    /// * `Address<dyn AcceptsTwo<u32, u64>>` -> `Address<dyn AcceptsOne<u64>>`.
    /// * `Address<dyn AcceptsOne<u64>>` -> `Address<dyn AcceptsNone>`.
    /// * `Address<Channel<MyProtocol>>` -> `Address<dyn AcceptsOne<u32>>`.
    pub fn transform<T>(self) -> DynAddress<T>
    where
        D: IntoDyn<T>,
    {
        self.transform_unchecked()
    }

    pub(crate) fn from_inner(address: InnerAddress<dyn BoxChannel>) -> Self {
        Self {
            address,
            phantom_data: PhantomData,
        }
    }

    /// Downcast a dynamic add
    pub fn downcast<T>(self) -> Result<StaticAddress<T>, Self>
    where
        T: Send + 'static,
    {
        match self.address.downcast::<T>() {
            Ok(address) => Ok(StaticAddress { address }),
            Err(address) => Err(Self {
                address,
                phantom_data: PhantomData,
            }),
        }
    }

    gen::dyn_send_methods!();
}

impl<P> Clone for StaticAddress<P> {
    fn clone(&self) -> Self {
        Self {
            address: self.address.clone(),
        }
    }
}

impl<D> Clone for DynAddress<D> {
    fn clone(&self) -> Self {
        Self {
            address: self.address.clone(),
            phantom_data: PhantomData,
        }
    }
}

//------------------------------------------------------------------------------------------------
//  Snd
//------------------------------------------------------------------------------------------------

pub struct Snd<'a, M: Message + 'static, P> {
    snd: SndRaw<'a, P>,
    rx: Option<<M::Type as MsgType<M>>::Returns>,
    phantom: PhantomData<M>,
}

impl<'a, M: Message + 'static, P> Snd<'a, M, P> {
    pub(crate) fn new(snd: SndRaw<'a, P>, rx: <M::Type as MsgType<M>>::Returns) -> Self {
        Self {
            snd,
            rx: Some(rx),
            phantom: PhantomData,
        }
    }
}

impl<'a, M, P> Unpin for Snd<'a, M, P>
where
    M: Message + 'static,
    P: Protocol + ProtocolMessage<M>,
{
}

impl<'a, M, P> Future for Snd<'a, M, P>
where
    M: Message + 'static,
    P: Protocol + ProtocolMessage<M>,
{
    type Output = Result<Returns<M>, SendError<M>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.snd
            .poll_unpin(cx)
            .map_err(|SendError(prot)| SendError(prot.unwrap_into_msg(self.rx.take().unwrap())))
            .map_ok(|()| self.rx.take().unwrap())
    }
}

use crate::*;
use event_listener::EventListener;

pub struct StaticAddress<P> {
    address: tiny_actor::Address<Channel<P>>,
}

impl<P> StaticAddress<P> {
    pub fn from_inner(address: tiny_actor::Address<Channel<P>>) -> Self {
        Self { address }
    }

    pub fn into_dyn<D>(self) -> DynAddress<D>
    where
        P: Protocol + IntoDyn<D>,
    {
        DynAddress::from_inner(self.address.transform_channel(|c| c))
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

impl<M, P> Accepts<M> for P
where
    P: AddressKind<Address = StaticAddress<P>> + ProtocolMessage<M>,
    Returns<M>: Send + 'static,
    Sends<M>: Send + 'static,
    M: Message + Send + 'static,
{
    fn try_send(address: &Self::Address, msg: M) -> Result<Returns<M>, TrySendError<M>> {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);

        match address.address.try_send(P::from_sends(sends)) {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendError::Closed(prot) => {
                    Err(TrySendError::Closed(prot.unwrap_into_msg(returns)))
                }
                TrySendError::Full(prot) => Err(TrySendError::Full(prot.unwrap_into_msg(returns))),
            },
        }
    }

    fn send_now(address: &Self::Address, msg: M) -> Result<Returns<M>, TrySendError<M>> {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);

        match address.address.send_now(P::from_sends(sends)) {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendError::Closed(prot) => {
                    Err(TrySendError::Closed(prot.unwrap_into_msg(returns)))
                }
                TrySendError::Full(prot) => Err(TrySendError::Full(prot.unwrap_into_msg(returns))),
            },
        }
    }

    fn send_blocking(address: &Self::Address, msg: M) -> Result<Returns<M>, SendError<M>> {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);

        match address.address.send_blocking(P::from_sends(sends)) {
            Ok(()) => Ok(returns),
            Err(SendError(prot)) => Err(SendError(prot.unwrap_into_msg(returns))),
        }
    }

    fn send(address: &Self::Address, msg: M) -> SendFut<'_, M> {
        Box::pin(async move {
            let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);

            match address.address.send(P::from_sends(sends)).await {
                Ok(()) => Ok(returns),
                Err(SendError(prot)) => Err(SendError(prot.unwrap_into_msg(returns))),
            }
        })
    }
}

impl<P> Clone for StaticAddress<P> {
    fn clone(&self) -> Self {
        Self {
            address: self.address.clone(),
        }
    }
}

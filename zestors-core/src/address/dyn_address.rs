use crate::*;
use event_listener::EventListener;
use std::{any::TypeId, marker::PhantomData};

pub struct DynAddress<D> {
    address: tiny_actor::Address<dyn BoxChannel>,
    phantom_data: PhantomData<D>,
}

impl<D> DynAddress<D> {
    pub fn transform_unchecked<T>(self) -> DynAddress<T> {
        DynAddress::from_inner(self.address)
    }

    pub fn transform<T>(self) -> DynAddress<T>
    where
        D: IntoDyn<T>,
    {
        self.transform_unchecked()
    }

    pub fn try_transform<T: IsDyn>(self) -> Result<DynAddress<T>, Self> {
        if T::message_ids().iter().all(|id| self.accepts(id)) {
            Ok(self.transform_unchecked())
        } else {
            Err(self)
        }
    }

    pub fn accepts(&self, id: &TypeId) -> bool {
        self.address.channel_ref().accepts(id)
    }

    pub(crate) fn from_inner(address: tiny_actor::Address<dyn BoxChannel>) -> Self {
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
            Ok(address) => Ok(StaticAddress::from_inner(address)),
            Err(address) => Err(Self {
                address,
                phantom_data: PhantomData,
            }),
        }
    }

    pub fn try_send_unchecked<M>(&self, msg: M) -> Result<Returns<M>, TrySendDynError<M>>
    where
        M: Message + Send + 'static,
        Sends<M>: Send + 'static,
    {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);
        let res = self
            .address
            .channel_ref()
            .try_send_boxed(BoxedMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendDynError::Full(boxed) => Err(TrySendDynError::Full(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
                TrySendDynError::Closed(boxed) => Err(TrySendDynError::Closed(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
                TrySendDynError::NotAccepted(boxed) => Err(TrySendDynError::NotAccepted(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
            },
        }
    }

    pub fn send_now_unchecked<M>(&self, msg: M) -> Result<Returns<M>, TrySendDynError<M>>
    where
        M: Message + Send + 'static,
        Sends<M>: Send + 'static,
    {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);
        let res = self
            .address
            .channel_ref()
            .send_now_boxed(BoxedMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendDynError::Full(boxed) => Err(TrySendDynError::Full(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
                TrySendDynError::Closed(boxed) => Err(TrySendDynError::Closed(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
                TrySendDynError::NotAccepted(boxed) => Err(TrySendDynError::NotAccepted(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
            },
        }
    }

    pub fn send_blocking_unchecked<M>(&self, msg: M) -> Result<Returns<M>, SendDynError<M>>
    where
        M: Message + Send + 'static,
        Sends<M>: Send + 'static,
    {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);
        let res = self
            .address
            .channel_ref()
            .send_blocking_boxed(BoxedMessage::new::<M>(sends));

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                SendDynError::Closed(boxed) => Err(SendDynError::Closed(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
                SendDynError::NotAccepted(boxed) => Err(SendDynError::NotAccepted(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
            },
        }
    }

    pub async fn send_unchecked<M>(&self, msg: M) -> Result<Returns<M>, SendDynError<M>>
    where
        M: Message + Send + 'static,
        Sends<M>: Send + 'static,
    {
        let (sends, returns) = <M::Type as MsgType<M>>::new_pair(msg);
        let res = self
            .address
            .channel_ref()
            .send_boxed(BoxedMessage::new::<M>(sends))
            .await;

        match res {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                SendDynError::Closed(boxed) => Err(SendDynError::Closed(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
                SendDynError::NotAccepted(boxed) => Err(SendDynError::NotAccepted(
                    boxed.downcast_into_msg(returns).unwrap(),
                )),
            },
        }
    }
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

impl<M, T> Accepts<M> for Dyn<T>
where
    Self: IntoDyn<Dyn<dyn AcceptsOne<M>>>,
    M: Message + Send + 'static,
    Sends<M>: Send + 'static,
    Returns<M>: Send + 'static,
    T: ?Sized,
{
    fn try_send(address: &DynAddress<Dyn<T>>, msg: M) -> Result<Returns<M>, TrySendError<M>> {
        address.try_send_unchecked(msg).map_err(|e| match e {
            TrySendDynError::Full(msg) => TrySendError::Full(msg),
            TrySendDynError::Closed(msg) => TrySendError::Closed(msg),
            TrySendDynError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    fn send_now(address: &Self::Address, msg: M) -> Result<Returns<M>, TrySendError<M>> {
        address.send_now_unchecked(msg).map_err(|e| match e {
            TrySendDynError::Full(msg) => TrySendError::Full(msg),
            TrySendDynError::Closed(msg) => TrySendError::Closed(msg),
            TrySendDynError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    fn send_blocking(address: &Self::Address, msg: M) -> Result<Returns<M>, SendError<M>> {
        address.send_blocking_unchecked(msg).map_err(|e| match e {
            SendDynError::Closed(msg) => SendError(msg),
            SendDynError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    fn send(address: &Self::Address, msg: M) -> SendFut<M> {
        Box::pin(async move {
            address.send_unchecked(msg).await.map_err(|e| match e {
                SendDynError::Closed(msg) => SendError(msg),
                SendDynError::NotAccepted(_) => {
                    panic!("Sent message which was not accepted by actor")
                }
            })
        })
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

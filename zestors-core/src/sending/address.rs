use crate::*;
use event_listener::EventListener;
use futures::{future::BoxFuture, Future, FutureExt};
use std::{
    any::TypeId,
    fmt::Debug,
    mem::ManuallyDrop,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

/// An address is a reference to the actor, used to send messages.
///
/// Addresses can be of two forms:
/// * `Address<Channel<M>>`: This is the default form, which can be used to send messages of
/// type `M`. It can be transformed into an `Address` using [Address::into_dyn].
/// * `Address`: This form is a dynamic address, which can do everything a normal address can
/// do, except for sending messages. It can be transformed back into an `Address<Channel<M>>` using
/// [`Address::downcast::<M>`].
///
/// An address can be awaited which returns once the actor exits.
#[derive(Debug)]
pub struct Address<C: DefineChannel = Accepts![]> {
    channel: Arc<C::Channel>,
    exit_listener: Option<EventListener>,
}

impl<C: DefineChannel> Address<C> {
    pub fn from_channel(channel: Arc<C::Channel>) -> Self {
        Self {
            channel,
            exit_listener: None,
        }
    }

    fn into_parts(self) -> (Arc<C::Channel>, Option<EventListener>) {
        let manually_drop = ManuallyDrop::new(self);
        unsafe {
            let channel = std::ptr::read(&manually_drop.channel);
            let exit_listener = std::ptr::read(&manually_drop.exit_listener);
            (channel, exit_listener)
        }
    }

    pub fn transform_unchecked_into<T>(self) -> Address<T>
    where
        T: DefineDynChannel,
    {
        let (channel, exit_listener) = self.into_parts();
        Address {
            channel: C::into_dyn_channel(channel),
            exit_listener,
        }
    }

    pub fn transform_into<T>(self) -> Address<T>
    where
        C: TransformInto<T>,
        T: DefineDynChannel,
    {
        self.transform_unchecked_into()
    }

    pub fn into_dyn(self) -> Address {
        self.transform_unchecked_into()
    }

    pub fn try_transform_into<T>(self) -> Result<Address<T>, Self>
    where
        T: DefineDynChannel,
        C: DefineDynChannel,
    {
        if C::msg_ids().iter().all(|id| self.accepts(id)) {
            Ok(self.transform_unchecked_into())
        } else {
            Err(self)
        }
    }

    pub fn downcast<T>(self) -> Result<Address<T>, Self>
    where
        T: DefineChannel,
        T::Channel: Sized + 'static,
        C: DefineDynChannel,
    {
        let (channel, exit_listener) = self.into_parts();
        match channel.clone().into_any().downcast() {
            Ok(channel) => Ok(Address {
                channel,
                exit_listener,
            }),
            Err(_) => Err(Self {
                channel,
                exit_listener,
            }),
        }
    }
}

impl<T: DefineChannel> ActorRef for Address<T> {
    type ChannelDefinition = T;
    fn close(&self) -> bool {
        self.channel.close()
    }
    fn halt_some(&self, n: u32) {
        self.channel.halt_some(n)
    }
    fn halt(&self) {
        self.channel.halt()
    }
    fn process_count(&self) -> usize {
        self.channel.process_count()
    }
    fn msg_count(&self) -> usize {
        self.channel.msg_count()
    }
    fn address_count(&self) -> usize {
        self.channel.address_count()
    }
    fn is_closed(&self) -> bool {
        self.channel.is_closed()
    }
    fn is_bounded(&self) -> bool {
        self.channel.capacity().is_bounded()
    }
    fn capacity(&self) -> &Capacity {
        self.channel.capacity()
    }
    fn has_exited(&self) -> bool {
        self.channel.has_exited()
    }
    fn actor_id(&self) -> ActorId {
        self.channel.actor_id()
    }
    fn try_send<M>(&self, msg: M) -> Result<Returned<M>, TrySendError<M>>
    where
        M: Message,
        Self::ChannelDefinition: Accept<M>,
    {
        <Self::ChannelDefinition as Accept<M>>::try_send(&self.channel, msg)
    }
    fn send_now<M>(&self, msg: M) -> Result<Returned<M>, TrySendError<M>>
    where
        M: Message,
        Self::ChannelDefinition: Accept<M>,
    {
        <Self::ChannelDefinition as Accept<M>>::send_now(&self.channel, msg)
    }
    fn send_blocking<M>(&self, msg: M) -> Result<Returned<M>, SendError<M>>
    where
        M: Message,
        Self::ChannelDefinition: Accept<M>,
    {
        <Self::ChannelDefinition as Accept<M>>::send_blocking(&self.channel, msg)
    }
    fn send<M>(&self, msg: M) -> <Self::ChannelDefinition as Accept<M>>::SendFut<'_>
    where
        M: Message,
        Self::ChannelDefinition: Accept<M>,
    {
        <Self::ChannelDefinition as Accept<M>>::send(&self.channel, msg)
    }

    fn get_address(&self) -> Address<Self::ChannelDefinition> {
        self.clone()
    }
}

impl<T: DefineDynChannel> DynActorRef for Address<T> {
    fn try_send_unchecked<M>(&self, msg: M) -> Result<Returned<M>, TrySendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        Sent<M>: Send + 'static,
    {
        self.channel.try_send_unchecked(msg)
    }
    fn send_now_unchecked<M>(&self, msg: M) -> Result<Returned<M>, TrySendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        Sent<M>: Send + 'static,
    {
        self.channel.send_now_unchecked(msg)
    }
    fn send_blocking_unchecked<M>(&self, msg: M) -> Result<Returned<M>, SendUncheckedError<M>>
    where
        M: Message + Send + 'static,
        Sent<M>: Send + 'static,
    {
        self.channel.send_blocking_unchecked(msg)
    }
    fn send_unchecked<M>(&self, msg: M) -> BoxFuture<'_, Result<Returned<M>, SendUncheckedError<M>>>
    where
        <M::Type as MessageType<M>>::Returned: Send,
        M: Message + Send + 'static,
        Sent<M>: Send + 'static,
    {
        self.channel.send_unchecked(msg)
    }
    fn accepts(&self, id: &TypeId) -> bool {
        self.channel.accepts(id)
    }
}

impl<T: DefineChannel> Future for Address<T> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.channel.has_exited() {
            Poll::Ready(())
        } else {
            if self.exit_listener.is_none() {
                self.exit_listener = Some(self.channel.get_exit_listener())
            }
            if self.channel.has_exited() {
                Poll::Ready(())
            } else {
                self.exit_listener.as_mut().unwrap().poll_unpin(cx)
            }
        }
    }
}

impl<T: DefineChannel> Unpin for Address<T> {}

impl<T: DefineChannel> Clone for Address<T> {
    fn clone(&self) -> Self {
        self.channel.add_address();
        Self {
            channel: self.channel.clone(),
            exit_listener: None,
        }
    }
}

impl<T: DefineChannel> Drop for Address<T> {
    fn drop(&mut self) {
        self.channel.remove_address();
    }
}

//------------------------------------------------------------------------------------------------
//  IntoAddress
//------------------------------------------------------------------------------------------------

// // todo: Make this like IntoChannel
// pub trait IntoAddress<T>
// where
//     T: DefineChannel,
// {
//     fn into_address(self) -> Address<T>;
// }

// impl<P, T> IntoAddress<T> for Address<P>
// where
//     P: Protocol + TransformInto<T>,
//     T: DefineDynChannel,
// {
//     fn into_address(self) -> Address<T> {
//         self.into_dyn()
//     }
// }

// impl<T, D> IntoAddress<T> for Address<Dyn<D>>
// where
//     T: DefineDynChannel,
//     D: ?Sized,
//     Dyn<D>: DefineDynChannel + TransformInto<T>,
// {
//     fn into_address(self) -> Address<T> {
//         self.transform_into()
//     }
// }

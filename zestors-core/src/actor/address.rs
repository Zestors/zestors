use crate::*;
use event_listener::EventListener;
use futures::{Future, FutureExt};
use std::{
    fmt::Debug,
    mem::ManuallyDrop,
    ops::Add,
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
/// [Address::downcast::<M>].
///
/// An address can be awaited which returns once the actor exits.
#[derive(Debug)]
pub struct Address<T: DefinesChannel = Accepts![]> {
    channel: Arc<T::Channel>,
    exit_listener: Option<EventListener>,
}

impl<T: DefinesChannel> Address<T> {
    /// Does not increment the address-count.
    pub(crate) fn new(channel: Arc<T::Channel>) -> Self {
        Self {
            channel,
            exit_listener: None,
        }
    }

    pub(crate) fn channel(&self) -> &Arc<T::Channel> {
        &self.channel
    }

    fn into_parts(self) -> (Arc<T::Channel>, Option<EventListener>) {
        let manually_drop = ManuallyDrop::new(self);
        unsafe {
            let channel = std::ptr::read(&manually_drop.channel);
            let exit_listener = std::ptr::read(&manually_drop.exit_listener);
            (channel, exit_listener)
        }
    }

    gen::channel_methods!();
    gen::send_methods!(T);
}

impl<T: DefinesDynChannel> Address<T> {
    pub fn transform_unchecked<T2>(self) -> Address<T2>
    where
        T2: DefinesDynChannel,
    {
        let (channel, exit_listener) = self.into_parts();
        Address {
            channel,
            exit_listener,
        }
    }

    pub fn transform<T2>(self) -> Address<T2>
    where
        T: TransformInto<T2>,
        T2: DefinesDynChannel,
    {
        self.transform_unchecked()
    }

    pub fn try_transform<T2>(self) -> Result<Address<T2>, Self>
    where
        T2: DefinesDynChannel,
    {
        if T::msg_ids().iter().all(|id| self.accepts(id)) {
            Ok(self.transform_unchecked())
        } else {
            Err(self)
        }
    }

    ///Whether the actor accepts a message of the given type-id.
    pub fn accepts(&self, id: &std::any::TypeId) -> bool {
        self.channel.accepts(id)
    }

    pub fn downcast<P: Protocol>(self) -> Result<Address<P>, Self> {
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
    gen::unchecked_send_methods!();
}

impl<P: Protocol> Address<P> {
    /// Convert the `Address<Channel<M>>` into an `Address`.
    pub fn into_dyn<T>(self) -> Address<T>
    where
        P: TransformInto<T>,
        T: DefinesDynChannel,
    {
        let (channel, exit_listener) = self.into_parts();
        Address {
            channel,
            exit_listener,
        }
    }
}

impl<T: DefinesChannel> Future for Address<T> {
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

impl<T: DefinesChannel> Unpin for Address<T> {}

impl<T: DefinesChannel> Clone for Address<T> {
    fn clone(&self) -> Self {
        self.channel.add_address();
        Self {
            channel: self.channel.clone(),
            exit_listener: None,
        }
    }
}

impl<T: DefinesChannel> Drop for Address<T> {
    fn drop(&mut self) {
        self.channel.remove_address();
    }
}

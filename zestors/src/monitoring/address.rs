use crate::all::*;
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
use zestors_core::actor_type::ActorType;

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
pub struct Address<A: ActorType = Accepts![]> {
    channel: Arc<A::Channel>,
    exit_listener: Option<EventListener>,
}

impl<A: ActorType> Address<A> {
    pub fn from_channel(channel: Arc<A::Channel>) -> Self {
        Self {
            channel,
            exit_listener: None,
        }
    }

    fn into_parts(self) -> (Arc<A::Channel>, Option<EventListener>) {
        let manually_drop = ManuallyDrop::new(self);
        unsafe {
            let channel = std::ptr::read(&manually_drop.channel);
            let exit_listener = std::ptr::read(&manually_drop.exit_listener);
            (channel, exit_listener)
        }
    }

    pub fn transform_unchecked_into<T>(self) -> Address<T>
    where
        T: DynActorType,
    {
        let (channel, exit_listener) = self.into_parts();
        Address {
            channel: <A::Channel as Channel>::into_dyn(channel),
            exit_listener,
        }
    }

    pub fn transform_into<T>(self) -> Address<T>
    where
        A: TransformInto<T>,
        T: ActorType,
    {
        let (channel, exit_listener) = self.into_parts();
        Address {
            channel: A::transform_into(channel),
            exit_listener,
        }
    }

    pub fn into_dyn(self) -> Address {
        self.transform_unchecked_into()
    }

    pub fn try_transform_into<T>(self) -> Result<Address<T>, Self>
    where
        A: DynActorType,
        T: DynActorType,
    {
        if T::msg_ids().iter().all(|id| self.accepts(id)) {
            Ok(self.transform_unchecked_into())
        } else {
            Err(self)
        }
    }

    pub fn downcast<T>(self) -> Result<Address<T>, Self>
    where
        A: DynActorType,
        T: ActorType,
        T::Channel: Sized + 'static,
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

impl<A: ActorType> ActorRef for Address<A> {
    type ActorType = A;
    fn channel(&self) -> &Arc<<Self::ActorType as ActorType>::Channel> {
        &self.channel
    }
}

impl<A: ActorType> Future for Address<A> {
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

impl<A: ActorType> Unpin for Address<A> {}

impl<A: ActorType> Clone for Address<A> {
    fn clone(&self) -> Self {
        self.channel.add_address();
        Self {
            channel: self.channel.clone(),
            exit_listener: None,
        }
    }
}

impl<T: ActorType> Drop for Address<T> {
    fn drop(&mut self) {
        self.channel.remove_address();
    }
}

// ------------------------------------------------------------------------------------------------
//  IntoAddress
// ------------------------------------------------------------------------------------------------

// todo: Make this like IntoChannel
pub trait IntoAddress<T>
where
    T: ActorType,
{
    fn into_address(self) -> Address<T>;
}

impl<A, T> IntoAddress<T> for Address<A>
where
    A: TransformInto<T>,
    T: ActorType,
{
    fn into_address(self) -> Address<T> {
        self.transform_into()
    }
}

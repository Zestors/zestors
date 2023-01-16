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
use zestors_core::actor_kind::ActorKind;

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
pub struct Address<A: ActorKind = Accepts![]> {
    channel: Arc<A::Channel>,
    exit_listener: Option<EventListener>,
}

impl<A: ActorKind> Address<A> {
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
        T: DynActorKind,
    {
        let (channel, exit_listener) = self.into_parts();
        Address {
            channel: A::into_dyn_channel(channel),
            exit_listener,
        }
    }

    pub fn transform_into<T>(self) -> Address<T>
    where
        A: TransformInto<T>,
        T: ActorKind,
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
        A: DynActorKind,
        T: DynActorKind,
    {
        if A::msg_ids().iter().all(|id| self.accepts(id)) {
            Ok(self.transform_unchecked_into())
        } else {
            Err(self)
        }
    }

    pub fn downcast<T>(self) -> Result<Address<T>, Self>
    where
        A: DynActorKind,
        T: ActorKind,
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

impl<A: ActorKind> ActorRef for Address<A> {
    type ActorKind = A;
    fn channel(actor_ref: &Self) -> &Arc<<Self::ActorKind as ActorKind>::Channel> {
        &actor_ref.channel
    }
}

// impl<A: DynActorKind> DynActorRefExt for Address<A> {
//     fn try_send_unchecked<M>(&self, msg: M) -> Result<M::Returned, TrySendUncheckedError<M>>
//     where
//         M: Message + Send + 'static,
//         M::Payload: Send + 'static,
//     {
//         self.channel.try_send_unchecked(msg)
//     }
//     fn send_now_unchecked<M>(&self, msg: M) -> Result<M::Returned, TrySendUncheckedError<M>>
//     where
//         M: Message + Send + 'static,
//         M::Payload: Send + 'static,
//     {
//         self.channel.send_now_unchecked(msg)
//     }
//     fn send_blocking_unchecked<M>(&self, msg: M) -> Result<M::Returned, SendUncheckedError<M>>
//     where
//         M: Message + Send + 'static,
//         M::Payload: Send + 'static,
//     {
//         self.channel.send_blocking_unchecked(msg)
//     }
//     fn send_unchecked<M>(&self, msg: M) -> BoxFuture<'_, Result<M::Returned, SendUncheckedError<M>>>
//     where
//         M::Returned: Send,
//         M: Message + Send + 'static,
//         M::Payload: Send + 'static,
//     {
//         self.channel.send_unchecked(msg)
//     }
//     fn accepts(&self, id: &TypeId) -> bool {
//         self.channel.accepts(id)
//     }
// }

impl<A: ActorKind> Future for Address<A> {
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

impl<A: ActorKind> Unpin for Address<A> {}

impl<A: ActorKind> Clone for Address<A> {
    fn clone(&self) -> Self {
        self.channel.add_address();
        Self {
            channel: self.channel.clone(),
            exit_listener: None,
        }
    }
}

impl<T: ActorKind> Drop for Address<T> {
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
    T: ActorKind,
{
    fn into_address(self) -> Address<T>;
}

impl<A, T> IntoAddress<T> for Address<A>
where
    A: TransformInto<T>,
    T: ActorKind,
{
    fn into_address(self) -> Address<T> {
        self.transform_into()
    }
}

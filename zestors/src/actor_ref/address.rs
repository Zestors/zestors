use crate::{all::*, DynActor};
use event_listener::EventListener;
use futures::{Future, FutureExt};
use std::{
    fmt::Debug,
    mem::ManuallyDrop,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

/// An address is a cloneable [reference](ActorRef) to an actor. Most methods are located in the traits
/// [`ActorRefExt`] and [`Transformable`].
///
/// ## ActorType
/// The generic parameter `A` in `Address<A>` defines the [`ActorType`].
///
/// An `Address` = `Address<Acceps![]>`. (An address that doesn't accept any messages).
///
/// ## Awaiting
/// An address can be awaited which resolves to `()` when the actor exits.
#[derive(Debug)]
pub struct Address<A: ActorType = DynActor!()> {
    channel: Arc<A::Channel>,
    exit_listener: Option<EventListener>,
}

impl<A: ActorType> Address<A> {
    pub(crate) fn from_channel(channel: Arc<A::Channel>) -> Self {
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
}

impl<A: ActorType> Transformable for Address<A> {
    type IntoRef<T> = Address<T> where T: ActorType;

    fn transform_unchecked_into<T>(self) -> Self::IntoRef<T>
    where
        T: DynActorType,
    {
        let (channel, exit_listener) = self.into_parts();
        Address {
            channel: <A::Channel as Channel>::into_dyn(channel),
            exit_listener,
        }
    }

    fn transform_into<T>(self) -> Self::IntoRef<T>
    where
        Self::ActorType: TransformInto<T>,
        T: ActorType,
    {
        let (channel, exit_listener) = self.into_parts();
        Address {
            channel: A::transform_into(channel),
            exit_listener,
        }
    }

    fn downcast<T>(self) -> Result<Self::IntoRef<T>, Self>
    where
        Self::ActorType: DynActorType,
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
    fn channel_ref(this: &Self) -> &Arc<<Self::ActorType as ActorType>::Channel> {
        &this.channel
    }
}

impl<A: ActorType> Future for Address<A> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        let exit_listener = this
            .exit_listener
            .get_or_insert(this.channel.get_exit_listener());

        if this.channel.has_exited() {
            Poll::Ready(())
        } else {
            exit_listener.poll_unpin(cx)
        }
    }
}

impl<A: ActorType> Unpin for Address<A> {}

impl<A: ActorType> Clone for Address<A> {
    fn clone(&self) -> Self {
        self.clone_address()
    }
}

impl<T: ActorType> Drop for Address<T> {
    fn drop(&mut self) {
        self.channel.decrement_address_count();
    }
}

//------------------------------------------------------------------------------------------------
//  IntoAddress
//------------------------------------------------------------------------------------------------

/// Implemented for any type that can be transformed into an [`Address<A>`].
pub trait IntoAddress<A: ActorType = DynActor!()> {
    fn into_address(self) -> Address<A>;
}

impl<A: ActorType, T: ActorType> IntoAddress<T> for Address<A>
where
    A: TransformInto<T>,
{
    fn into_address(self) -> Address<T> {
        self.transform_into()
    }
}

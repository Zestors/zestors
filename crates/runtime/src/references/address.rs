use crate::*;
use std::{
    any::TypeId, convert::Infallible, fmt::Debug, hash::Hash, marker::PhantomData, sync::Arc,
};

/// A weak reference to an actor's channel, which can be used to send
/// messages and signals to it without keeping it alive.
///
/// Unlike [`StrongAddress`] (and the [`Inbox`]/[`Child`] built on top of it),
/// an `Address` does not count toward the actor's strong reference count:
/// once every strong reference is dropped, the actor is permanently gone even
/// while `Address`es to it still exist. Use [`ActorOps::upgrade`] to attempt
/// to obtain a [`StrongAddress`] from an `Address`.
///
/// Once all strong references to the channel are dropped, the actor is
/// deregistered from the local [`Registry`] and the channel is dropped. This
/// means that in order to restart an actor, a [`StrongAddress`] (or an
/// [`Inbox`]/[`Child`] holding one) must be kept alive.
#[repr(transparent)]
pub struct Address<C: Context = Dyn> {
    inner: Arc<Channel>,
    _ctx: PhantomData<fn() -> C>,
}

impl<C: Context> Address<C> {
    pub(crate) fn _clone(&self) -> Self {
        Self {
            _ctx: PhantomData,
            inner: self.inner.clone(),
        }
    }

    pub(crate) fn _channel(&self) -> &Arc<Channel> {
        &self.inner
    }
}

impl<I: Interface> Address<I> {
    pub(crate) fn new(pid: Pid, strong_count: usize) -> Self {
        let msg_queue_capacity = match TypeId::of::<I>() == TypeId::of::<Infallible>() {
            true => 1,
            false => MSG_QUEUE_CAPACITY,
        };

        let inner: Arc<Channel> = Arc::new(Channel::new(
            pid,
            strong_count,
            ConcurrentQueue::<I>::bounded(msg_queue_capacity),
        ));

        Self {
            inner,
            _ctx: PhantomData,
        }
    }
}

impl<C: Context> ActorRef for Address<C> {
    type Ctx = C;

    fn actor_ref(&self) -> &Address<Self::Ctx> {
        self
    }
}

impl<C: Context> IntoDyn for Address<C> {
    type Ref<R: Context> = Address<R>;

    fn into_context_unchecked<R>(self) -> Self::Ref<R>
    where
        R: Context,
    {
        Address {
            inner: self.inner,
            _ctx: PhantomData,
        }
    }
}

impl<C: Context> AsDyn for Address<C> {
    fn as_context_unchecked<S>(&self) -> &Self::Ref<S>
    where
        S: Context,
    {
        // SAFETY: Same memory layout
        unsafe { &*(self as *const Address<C> as *const Address<S>) }
    }
}

impl<T: Context> Clone for Address<T> {
    fn clone(&self) -> Self {
        self._clone()
    }
}

impl<C: Context> Debug for Address<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Address")
            .field("pid", &self._channel().pid())
            .field("status", &self._channel().status())
            .field("len", &self._channel().msg_len())
            .finish()
    }
}

impl<C: Context> Eq for Address<C> {}
impl<C: Context> PartialEq for Address<C> {
    fn eq(&self, other: &Self) -> bool {
        self._channel().pid() == other._channel().pid()
    }
}
impl<C: Context> Hash for Address<C> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self._channel().pid().hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Interface)]
    #[interface(path = "zestors_interface")]
    pub enum MyInterface {
        A(Envelope<u32>),
        AB(Envelope<u64>),
    }

    #[tokio::test]
    async fn test_address_downcast_ref() {
        let child = crate::spawn_rand(|_: Inbox<MyInterface>| async move { Ok(()) });
        let address = child.address().clone().into_dyn::<()>();

        address
            .downcast::<MyInterface>()
            .expect("Should downcast to MyInterface");
    }
}

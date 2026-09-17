use crate::registry::Registry;
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
    inner: Arc<Channel<dyn DynamicQueue>>,
    _ctx: PhantomData<fn() -> C>,
}

impl<C: Context> Address<C> {
    pub(crate) fn _clone(&self) -> Self {
        Self {
            _ctx: PhantomData,
            inner: self.inner.clone(),
        }
    }

    pub(crate) fn decr_strong_count(&self) {
        if self.data().decr_strong_count() {
            let removed_address = Registry::local().remove(self.pid());

            if removed_address.is_none() {
                if cfg!(debug_assertions) {
                    panic!(
                        "Address {} was not found in the registry when dropping the last strong reference",
                        self.pid()
                    );
                } else {
                    tracing::error!(
                        "Address {} was not found in the registry when dropping the last strong reference",
                        self.pid()
                    );
                }
            }
        }
    }

    pub(crate) fn incr_strong_count(&self) {
        self.data().incr_strong_count();
    }

    pub(crate) fn data(&self) -> &Channel<dyn DynamicQueue> {
        &self.inner
    }

    pub(crate) fn try_push_msg<M: Message>(&self, msg: M) -> Result<M::Receipt, NotAccepted<M>> {
        self.data().try_push_msg(msg)
    }

    pub(crate) fn msg_notify_one(&self) {
        self.data().msg_notify_one();
    }

    #[expect(unused)]
    pub(crate) fn pop_dyn(&self) -> Result<AnyEnvelope, PopError> {
        self.data().pop_dyn()
    }

    pub(crate) fn register_spawned(&self) -> Result<(), InvalidStatusUpdate> {
        self.data().register_spawned()
    }

    pub(crate) fn register_exited(&self, reason: Result<(), ExitError>) -> bool {
        self.data().register_exited(reason)
    }

    pub(crate) fn register_initialized(&self) -> Result<bool, InvalidStatusUpdate> {
        self.data().register_initialized()
    }

    pub(crate) fn register_exiting(&self) -> Result<bool, InvalidStatusUpdate> {
        self.data().register_exiting()
    }

    pub(crate) fn raw_queue(&self) -> Option<&ConcurrentQueue<C>>
    where
        C: Interface,
    {
        self.data().raw_queue::<C>()
    }

    pub(crate) fn backpressure(&self) -> &BackPressure {
        BackPressure::global()
    }

    pub(crate) async fn delay_for_backpressure(&self) {
        self.data().delay_for_backpressure().await
    }

    pub(crate) fn ref_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
}

impl<I: Interface> Address<I> {
    pub(crate) fn new(pid: Pid, strong_count: usize) -> Self {
        let msg_queue_capacity = match TypeId::of::<I>() == TypeId::of::<Infallible>() {
            true => 1,
            false => MSG_QUEUE_CAPACITY,
        };

        let inner: Arc<Channel<dyn DynamicQueue>> = Arc::new(Channel::new(
            pid,
            strong_count,
            ConcurrentQueue::<I>::bounded(msg_queue_capacity),
        ));

        Self {
            inner,
            _ctx: PhantomData,
        }
    }

    pub(crate) async fn next_msg(&self) -> Option<I> {
        self.data().next_msg::<I>().await
    }

    pub(crate) fn pop_msg(&self) -> Option<I> {
        self.data().pop_msg::<I>()
    }

    pub(crate) fn drain_messages_and_signals(&self) {
        while let Some(msg) = self.pop_msg() {
            drop(msg);
        }

        while let Some(signal) = self.pop_signal() {
            let _ = signal;
        }
    }

    pub(crate) async fn next_signal(&self) -> Option<Signal> {
        self.data().next_signal().await
    }

    pub(crate) fn pop_signal(&self) -> Option<Signal> {
        self.data().pop_signal()
    }

    pub(crate) async fn next_event(&self, while_exiting: bool) -> Option<InboxEvent<I>> {
        match self.status() {
            ActorStatus::Suspended => self.next_signal().await.map(InboxEvent::Signal),

            ActorStatus::Exited(_) if self.msg_is_empty() => None,

            ActorStatus::Exiting if self.msg_is_empty() && !while_exiting => None,

            _ => {
                tokio::select! {
                    biased;

                    Some(signal) = self.next_signal() => Some(InboxEvent::Signal(signal)),
                    Some(msg) = self.next_msg() => Some(InboxEvent::Message(msg)),
                    else => None,
                }
            }
        }
    }

    pub(crate) fn try_next_event(&self) -> Option<InboxEvent<I>> {
        match self.status() {
            ActorStatus::Suspended => self.pop_signal().map(InboxEvent::Signal),
            ActorStatus::Exited(_) if self.msg_is_empty() => None,
            ActorStatus::Exiting if self.msg_is_empty() => None,
            _ => {
                if let Some(signal) = self.pop_signal() {
                    Some(InboxEvent::Signal(signal))
                } else if let Some(msg) = self.pop_msg() {
                    Some(InboxEvent::Message(msg))
                } else {
                    None
                }
            }
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

impl<T: Context> Clone for Address<T> {
    fn clone(&self) -> Self {
        self._clone()
    }
}

impl<C: Context> Debug for Address<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Address")
            .field("pid", &self.data().pid())
            .field("status", &self.data().status())
            .field("len", &self.data().msg_len())
            .finish()
    }
}

impl<C: Context> Eq for Address<C> {}
impl<C: Context> PartialEq for Address<C> {
    fn eq(&self, other: &Self) -> bool {
        self.data().pid() == other.data().pid()
    }
}
impl<C: Context> Hash for Address<C> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.data().pid().hash(state);
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

use crate::all::*;
use futures::future::BoxFuture;
use std::{any::TypeId, marker::PhantomData, sync::Arc};

/// An actor's [`ActorType`] specifies the [`Channel`] of the actor.
/// The [`ActorType`] can be one of the following:
/// - An sized [`InboxType`].
/// - A [`Dyn`] type, usually written with the [`Accepts!`] macro.
pub trait ActorType {
    type Channel: Channel + ?Sized;
}

/// A specialization of [`ActorType`] for a dynamically specified channel.
pub trait DynActorType: ActorType<Channel = dyn Channel> {
    fn msg_ids() -> Box<[TypeId]>;
}

/// Anything that can be passed along as the argument to the spawn function.
/// An inbox also defines the [`ActorType`] of spawned actor.
pub trait InboxType: ActorType + Send + 'static {
    type Config;

    /// Sets up the channel with the given address count and actor-id.
    fn setup_channel(
        config: Self::Config,
        address_count: usize,
        actor_id: ActorId,
    ) -> Arc<Self::Channel>;

    /// Creates another inbox from the channel, without adding anything to its process count.
    fn from_channel(channel: Arc<Self::Channel>) -> Self;
}

/// An [`InboxType`] that allows for spawning multiple processes onto an actor.
pub trait MultiInboxType: InboxType {
    /// Sets up the channel, preparing for x processes to be spawned.
    fn setup_multi_channel(
        config: Self::Config,
        process_count: usize,
        address_count: usize,
        actor_id: ActorId,
    ) -> Arc<Self::Channel>;
}

/// Allows for the transformation of one [`ActorType`] into another.
pub trait TransformInto<A: ActorType>: ActorType {
    fn transform_into(channel: Arc<Self::Channel>) -> Arc<A::Channel>;
}

/// A [`DynActorType`] with that accepts specific messages:
/// - `Dyn<dyn AcceptsNone>` -> Accepts no messages
/// - `Dyn<dyn AcceptsTwo<u32, u64>` -> Accepts two messages: `u32` and `u64`.
///
/// Any [InboxType] which accepts these messages can be transformed into a dynamic channel using
/// [TransformInto].
#[derive(Debug)]
pub struct Dyn<T: ?Sized>(PhantomData<*const T>);

unsafe impl<T: ?Sized> Send for Dyn<T> {}
unsafe impl<T: ?Sized> Sync for Dyn<T> {}

impl<D: ?Sized> ActorType for Dyn<D> {
    type Channel = dyn Channel;
}

impl<M, D> Accept<M> for Dyn<D>
where
    Self: DynActorType + TransformInto<Accepts![M]>,
    M: Message + Send + 'static,
    M::Payload: Send,
    M::Returned: Send,
    D: ?Sized,
{
    fn try_send(channel: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>> {
        channel.try_send_checked(msg).map_err(|e| match e {
            TrySendCheckedError::Full(msg) => TrySendError::Full(msg),
            TrySendCheckedError::Closed(msg) => TrySendError::Closed(msg),
            TrySendCheckedError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    fn force_send(channel: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>> {
        channel.force_send_unchecked(msg).map_err(|e| match e {
            TrySendCheckedError::Full(msg) => TrySendError::Full(msg),
            TrySendCheckedError::Closed(msg) => TrySendError::Closed(msg),
            TrySendCheckedError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    fn send_blocking(channel: &Self::Channel, msg: M) -> Result<M::Returned, SendError<M>> {
        channel.send_blocking_checked(msg).map_err(|e| match e {
            SendCheckedError::Closed(msg) => SendError(msg),
            SendCheckedError::NotAccepted(_) => {
                panic!("Sent message which was not accepted by actor")
            }
        })
    }

    type SendFut<'a> = BoxFuture<'a, Result<M::Returned, SendError<M>>>;

    fn send(channel: &Self::Channel, msg: M) -> Self::SendFut<'_> {
        Box::pin(async move {
            channel.send_checked(msg).await.map_err(|e| match e {
                SendCheckedError::Closed(msg) => SendError(msg),
                SendCheckedError::NotAccepted(_) => {
                    panic!("Sent message which was not accepted by actor")
                }
            })
        })
    }
}

#[cfg(test)]
mod test {
    use crate::Accepts;

    #[test]
    fn dynamic_definitions_compile() {
        type _1 = Accepts![()];
        type _2 = Accepts![(), ()];
        type _3 = Accepts![(), (), ()];
        type _4 = Accepts![(), (), (), ()];
        type _5 = Accepts![(), (), (), (), ()];
        type _6 = Accepts![(), (), (), (), (), ()];
        type _7 = Accepts![(), (), (), (), (), (), ()];
        type _8 = Accepts![(), (), (), (), (), (), (), ()];
        type _9 = Accepts![(), (), (), (), (), (), (), (), ()];
        type _10 = Accepts![(), (), (), (), (), (), (), (), (), ()];
    }
}

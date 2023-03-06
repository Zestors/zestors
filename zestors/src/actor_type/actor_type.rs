use crate::all::*;
use futures::future::BoxFuture;
use std::{any::TypeId, marker::PhantomData, sync::Arc};

/// The [`ActorType`] defines what kind of inbox the actor uses. An actor-type can
/// either be statically or dynamically typed:
/// - __Static__: A static actor-type is defined as the [`InboxType`].
/// - __Dynamic__: A dynamic actor-type is defined as a [`DynActor<dyn _>`], usually written
/// as [`DynActor!(Msg1, Msg2, ..]`)(DynActor!).
///
/// The actor-type is used as the generic parameter `A` in for example a [`Child<_, A, _>`] and
/// an [`Address<A>`]. These can then be `transformed` into and between dynamic actor-types.
pub trait ActorType {
    /// The unerlying [`Channel`] that this actor uses.
    type Channel: Channel + ?Sized;
}

/// A specialization of [`ActorType`] for dynamic actor-types.
pub trait DynActorType: ActorType<Channel = dyn Channel> {
    /// Get all [`Message`] type-ids that this actor accepts.
    fn msg_ids() -> Box<[TypeId]>;
}

/// All actors are spawned with an [`InboxType`] which defines the [`ActorType`] of that actor.
pub trait InboxType: ActorType + ActorRef<ActorType = Self> + Send + 'static {
    /// The inbox's configuration.
    type Config: Default + Send;

    /// Sets up the channel with the given address-count and actor-id.
    fn init_single_inbox(
        config: Self::Config,
        address_count: usize,
        actor_id: ActorId,
    ) -> (Arc<Self::Channel>, Self);
}

/// An [`InboxType`] that allows for spawning multiple processes onto an actor.
pub trait MultiProcessInbox: InboxType {
    /// Sets up the channel, preparing for x processes to be spawned.
    fn init_multi_inbox(
        config: Self::Config,
        process_count: usize,
        address_count: usize,
        actor_id: ActorId,
    ) -> Arc<Self::Channel>;

    /// Create an inbox from the channel.
    ///
    /// The inbox-count should be incremented outside of this method.
    fn from_channel(channel: Arc<Self::Channel>) -> Self;
}

/// A dynamic [`ActorType`] that accepts specific messages:
/// - `DynActor<dyn AcceptsNone>` -> Accepts no messages.
/// - `DynActor<dyn AcceptsTwo<u32, u64>` -> Accepts two messages: `u32` and `u64`.
///
/// See [`DynActor!`] for writing this types in a simpler way.
#[derive(Debug)]
pub struct DynActor<T: ?Sized>(PhantomData<fn() -> T>);


impl<T: ?Sized> ActorType for DynActor<T> {
    type Channel = dyn Channel;
}

impl<M, T: ?Sized> Accepts<M> for DynActor<T>
where
    Self: DynActorType + TransformInto<DynActor!(M)>,
    M: Message + Send + 'static,
    M::Returned: Send,
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
        channel.force_send_checked(msg).map_err(|e| match e {
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

/// Allows for the transformation of one [`ActorType`]'s [`Channel`] into another one's.
pub trait TransformInto<A: ActorType>: ActorType {
    /// Transform the channel into another one.
    fn transform_into(channel: Arc<Self::Channel>) -> Arc<A::Channel>;
}

#[cfg(test)]
mod test {
    use crate::DynActor;

    #[test]
    fn dynamic_definitions_compile() {
        type _1 = DynActor!(());
        type _2 = DynActor!((), ());
        type _3 = DynActor!((), (), ());
        type _4 = DynActor!((), (), (), ());
        type _5 = DynActor!((), (), (), (), ());
        type _6 = DynActor!((), (), (), (), (), ());
        type _7 = DynActor!((), (), (), (), (), (), ());
        type _8 = DynActor!((), (), (), (), (), (), (), ());
        type _9 = DynActor!((), (), (), (), (), (), (), (), ());
        type _10 = DynActor!((), (), (), (), (), (), (), (), (), ());
    }
}

use crate::all::*;
use futures::{future::BoxFuture, Future};
use std::{any::TypeId, sync::Arc};

/// This is the main trait for interacting with an actor, and is implemented for any
/// [`InboxType`], [`Child`] and [`Address`].
///
/// See [`ActorRefExt`] and [`Transformable`] for methods to call on an actor-reference.
pub trait ActorRef: Sized {
    /// The actor-type this refers to
    type ActorType: ActorType;

    /// Get a reference to the underlying channel.
    ///
    /// # Warning
    /// A [`Channel`] allows you to modify internal details of the actor. Only use this method
    /// if you know what you are doing. For regular use see [`ActorRefExt`].
    fn channel_ref(actor_ref: &Self) -> &Arc<<Self::ActorType as ActorType>::Channel>;
}

/// This is the main trait for interacting with an actor, and is automatically implemented for any type that
/// implements [`ActorRef`].
pub trait ActorRefExt: ActorRef {
    /// Create an [`struct@Envelope`] with the given message for this actor that can be sent later.
    fn envelope<M>(&self, msg: M) -> Envelope<'_, Self::ActorType, M>
    where
        M: Message,
        Self::ActorType: Accepts<M>,
    {
        Envelope::new(Self::channel_ref(self), msg)
    }

    /// Get an [`Address`] that points to this actor.
    fn clone_address(&self) -> Address<Self::ActorType> {
        let channel = Self::channel_ref(self).clone();
        channel.increment_address_count();
        Address::from_channel(channel)
    }

    /// Close the channel of this actor.
    fn close(&self) -> bool {
        Self::channel_ref(self).close()
    }

    /// Halt `n` processes of this actor.
    fn halt_some(&self, n: u32) {
        Self::channel_ref(self).halt_some(n)
    }

    /// Halt all processes of this actor.
    fn halt(&self) {
        Self::channel_ref(self).halt()
    }

    /// The amount of processes of this actor.
    fn process_count(&self) -> usize {
        Self::channel_ref(self).process_count()
    }

    /// The amount of messages in this actor's inbox.
    fn msg_count(&self) -> usize {
        Self::channel_ref(self).msg_count()
    }

    /// The amount of addresses pointing to this actor.
    fn address_count(&self) -> usize {
        Self::channel_ref(self).address_count()
    }

    /// Whether this channel is closed.
    fn is_closed(&self) -> bool {
        Self::channel_ref(self).is_closed()
    }

    /// The [`Capacity`] of this channel.
    fn capacity(&self) -> Capacity {
        Self::channel_ref(self).capacity()
    }

    /// Whether all processes of this actor have exited.
    /// (i.e. whether all inboxes have been dropped)
    fn has_exited(&self) -> bool {
        Self::channel_ref(self).has_exited()
    }

    /// The [`ActorId`] of this actor.
    fn actor_id(&self) -> ActorId {
        Self::channel_ref(self).actor_id()
    }

    /// Attempt to send a message to this actor. 
    ///
    /// If the inbox is full or if a [`BackPressure`]-timeout is returned this method will fail
    /// with [`TrySendError::Full`].
    fn try_send<M>(&self, msg: M) -> Result<M::Returned, TrySendError<M>>
    where
        M: Message,
        Self::ActorType: Accepts<M>,
    {
        <Self::ActorType as Accepts<M>>::try_send(Self::channel_ref(self), msg)
    }

    /// Attempt to send a message to this actor. 
    ///
    /// This method ignores any [`BackPressure`], but if the inbox is full this method will fail
    /// with [`TrySendError::Full`].
    fn force_send<M>(&self, msg: M) -> Result<M::Returned, TrySendError<M>>
    where
        M: Message,
        Self::ActorType: Accepts<M>,
    {
        <Self::ActorType as Accepts<M>>::force_send(Self::channel_ref(self), msg)
    }

    /// Attempt to send a message to this actor.
    ///
    /// Same as [`Self::send`] but blocks the thread.
    fn send_blocking<M>(&self, msg: M) -> Result<M::Returned, SendError<M>>
    where
        M: Message,
        Self::ActorType: Accepts<M>,
    {
        <Self::ActorType as Accepts<M>>::send_blocking(Self::channel_ref(self), msg)
    }

    /// Attempt to send a message to this actor.
    ///
    /// This method will wait until there is space in the channel or until the
    /// [`BackPressure`]-timeout is over.
    fn send<M>(&self, msg: M) -> <Self::ActorType as Accepts<M>>::SendFut<'_>
    where
        M: Message,
        Self::ActorType: Accepts<M>,
    {
        <Self::ActorType as Accepts<M>>::send(Self::channel_ref(self), msg)
    }

    /// [`try_send`](`Self::try_send`) a message to this actor and wait for the reply.
    fn try_request<M, F, E, R>(&self, msg: M) -> BoxFuture<'_, Result<R, TryRequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
        Self::ActorType: Accepts<M>,
    {
        <Self::ActorType as AcceptsExt<M>>::try_request(Self::channel_ref(self), msg)
    }

    /// [`force_send`](`Self::force_send`) a message to this actor and wait for the reply.
    fn force_request<M, F, E, R>(&self, msg: M) -> BoxFuture<'_, Result<R, TryRequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
        Self::ActorType: Accepts<M>,
    {
        <Self::ActorType as AcceptsExt<M>>::force_request(Self::channel_ref(self), msg)
    }

    /// [`send`](`Self::send`) a message to this actor and wait for the reply.
    fn request<M, F, E, R>(&self, msg: M) -> BoxFuture<'_, Result<R, RequestError<M, E>>>
    where
        M: Message<Returned = F> + Send + 'static,
        F: Future<Output = Result<R, E>> + Send,
        Self::ActorType: Accepts<M>,
    {
        <Self::ActorType as AcceptsExt<M>>::request(Self::channel_ref(self), msg)
    }

    /// Same as [`try_send`](`Self::try_send`), but checks at runtime that the message
    /// is actually accepted by the actor.
    fn try_send_checked<M>(&self, msg: M) -> Result<M::Returned, TrySendCheckedError<M>>
    where
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
        Self::ActorType: DynActorType,
    {
        <Self as ActorRef>::channel_ref(self).try_send_checked(msg)
    }

    /// Same as [`force_send`](`Self::force_send`), but checks at runtime that the message
    /// is actually accepted by the actor. 
    fn force_send_checked<M>(&self, msg: M) -> Result<M::Returned, TrySendCheckedError<M>>
    where
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
        Self::ActorType: DynActorType,
    {
        <Self as ActorRef>::channel_ref(self).force_send_checked(msg)
    }

    /// Same as [`send_blocking`](`Self::send_blocking`), but checks at runtime that the message
    /// is actually accepted by the actor. 
    fn send_blocking_checked<M>(&self, msg: M) -> Result<M::Returned, SendCheckedError<M>>
    where
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
        Self::ActorType: DynActorType,
    {
        <Self as ActorRef>::channel_ref(self).send_blocking_checked(msg)
    }

    /// Same as [`send`](`Self::send`), but checks at runtime that the message
    /// is actually accepted by the actor.
    fn send_checked<M>(&self, msg: M) -> BoxFuture<'_, Result<M::Returned, SendCheckedError<M>>>
    where
        M::Returned: Send,
        M: Message + Send + 'static,
        M::Payload: Send + 'static,
        Self::ActorType: DynActorType,
    {
        <Self as ActorRef>::channel_ref(self).send_checked(msg)
    }

    /// Whether the actor accepts a [`Message`] of this type-id.
    fn accepts(&self, id: &TypeId) -> bool {
        <Self as ActorRef>::channel_ref(self).accepts(id)
    }
}
impl<T> ActorRefExt for T where T: ActorRef {}

/// A trait implemented for all [actor-references][`ActorRef`] that can be transformed into and from dynamic
/// ones.
pub trait Transformable: ActorRef {
    /// The output-type of transformations.
    type IntoRef<T>: Transformable<ActorType = T>
    where
        T: ActorType;

    /// Transform the [`ActorType`] into a dynamic one, without checking if the actor accepts these messages.
    fn transform_unchecked_into<T>(self) -> Self::IntoRef<T>
    where
        T: DynActorType;

    /// Transform the [`ActorType`] into a dynamic one, checking at compile-time if the actor accepts these messages.
    fn transform_into<T>(self) -> Self::IntoRef<T>
    where
        Self::ActorType: TransformInto<T>,
        T: ActorType;

    /// Attempt to downcast the dynamic actor-type into an [`InboxType`].
    fn downcast<T>(self) -> Result<Self::IntoRef<T>, Self>
    where
        Self::ActorType: DynActorType,
        T: ActorType,
        T::Channel: Sized + 'static;

    /// Try to transform the [`ActorType`] into a dynamic one, checking at runtime if the actor accepts these messages.
    fn try_transform_into<T>(self) -> Result<Self::IntoRef<T>, Self>
    where
        Self::ActorType: DynActorType,
        T: DynActorType,
    {
        if T::msg_ids().iter().all(|id| self.accepts(id)) {
            Ok(self.transform_unchecked_into())
        } else {
            Err(self)
        }
    }

    /// Transform the [`ActorType`] into [`DynActor!(]`)(DynActor!).
    fn into_dyn(self) -> Self::IntoRef<DynActor!()> {
        self.transform_unchecked_into()
    }
}

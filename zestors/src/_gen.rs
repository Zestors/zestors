macro_rules! send_methods {
    ($at:ident) => {
        /// Attempts to send a message to an actor. If the mailbox is full or if a timeout is
        /// returned this method will fail.
        pub fn try_send<M>(&self, msg: M) -> Result<Returned<M>, tiny_actor::TrySendError<M>>
        where
            M: Message,
            T: Accepts<M>,
        {
            <T as Accepts<M>>::try_send(&self.$at.channel_ref(), msg)
        }

        /// Attempts to send a message to an actor. If the mailbox is full this will fail, but if
        /// a timeout is returned then this will succeed.
        pub fn send_now<M>(&self, msg: M) -> Result<Returned<M>, tiny_actor::TrySendError<M>>
        where
            M: Message,
            T: Accepts<M>,
        {
            <T as Accepts<M>>::send_now(&self.$at.channel_ref(), msg)
        }

        /// Same as `send`, but blocks the thread instead.
        pub fn send_blocking<M>(&self, msg: M) -> Result<Returned<M>, tiny_actor::SendError<M>>
        where
            M: Message,
            T: Accepts<M>,
        {
            <T as Accepts<M>>::send_blocking(&self.$at.channel_ref(), msg)
        }

        /// Send a message to the actor. If the inbox is full or returns a timeout, wait for this
        /// and then send it.
        pub fn send<M>(&self, msg: M) -> SendFut<M>
        where
            M: Message,
            T: Accepts<M>,
        {
            <T as Accepts<M>>::send(&self.$at.channel_ref(), msg)
        }
    };
}

pub(crate) use send_methods;

macro_rules! unchecked_send_methods {
    ($at:ident) => {
        /// Send a message to an actor without checking whether this actor accepts the message.
        ///
        /// If the actor does not accept the message, then nothing is sent and an error is returned.
        pub fn try_send_unchecked<M>(&self, msg: M) -> Result<Returned<M>, TrySendUncheckedError<M>>
        where
            M: Message + Send + 'static,
            Sent<M>: Send + 'static,
        {
            self.$at.channel_ref().try_send_unchecked(msg)
        }

        /// Send a message to an actor without checking whether this actor accepts the message.
        ///
        /// If the actor does not accept the message, then nothing is sent and an error is returned.
        pub fn send_now_unchecked<M>(&self, msg: M) -> Result<Returned<M>, TrySendUncheckedError<M>>
        where
            M: Message + Send + 'static,
            Sent<M>: Send + 'static,
        {
            self.$at.channel_ref().send_now_unchecked(msg)
        }

        /// Send a message to an actor without checking whether this actor accepts the message.
        ///
        /// If the actor does not accept the message, then nothing is sent and an error is returned.
        pub fn send_blocking_unchecked<M>(
            &self,
            msg: M,
        ) -> Result<Returned<M>, SendUncheckedError<M>>
        where
            M: Message + Send + 'static,
            Sent<M>: Send + 'static,
        {
            self.$at.channel_ref().send_blocking_unchecked(msg)
        }

        /// Send a message to an actor without checking whether this actor accepts the message.
        ///
        /// If the actor does not accept the message, then nothing is sent and an error is returned.
        pub async fn send_unchecked<M>(&self, msg: M) -> Result<Returned<M>, SendUncheckedError<M>>
        where
            M: Message + Send + 'static,
            Sent<M>: Send + 'static,
        {
            self.$at.channel_ref().send_unchecked(msg).await
        }
    };
}

macro_rules! transform_methods {
    ($at:ident, $ty:ty) => {
        /// Transform a dynamic-actor of one type into another type without checking if the
        /// actor accepts the messages.
        ///
        /// In most cases it is recomended to use `transform` and `try_transform` instead.
        pub fn transform_unchecked<T>(self) -> $ty
        where
            T: ActorType<Channel = dyn DynChannel>,
        {
            <$ty>::from_inner(self.$at)
        }

        /// Transfor a dynamic actor of one type into another type, while checking at compile
        /// time if the actor accepts all messages.
        pub fn transform<T>(self) -> $ty
        where
            D: IntoDyn<T>,
            T: ActorType<Channel = dyn DynChannel>,
        {
            self.transform_unchecked()
        }

        /// Transfor a dynamic actor of one type into another type, while checking at runtime
        /// if the actor accepts all messages.
        ///
        /// Wherever possible use `transform` instead.
        pub fn try_transform<T>(self) -> Result<$ty, Self>
        where
            T: IsDyn,
            T: ActorType<Channel = dyn DynChannel>,
        {
            if T::msg_ids().iter().all(|id| self.accepts(id)) {
                Ok(self.transform_unchecked())
            } else {
                Err(self)
            }
        }

        /// Whether the actor accepts a message of the given type-id.
        pub fn accepts(&self, id: &TypeId) -> bool {
            self.$at.channel_ref().accepts(id)
        }

        /// Downcast the dynamic actor back into the static actor given a protocol `T`.
        pub fn downcast<T>(self) -> Result<$ty, Self>
        where
            T: Protocol,
        {
            match self.$at.downcast::<T>() {
                Ok($at) => Ok(<$ty>::from_inner($at)),
                Err($at) => Err(Self { $at }),
            }
        }
    };
}

macro_rules! into_dyn_methods {
    ($at:ident, $ty:ty) => {
        /// Convert the static actor into a dynamic one, checked at compile-time.
        pub fn into_dyn<T>(self) -> $ty
        where
            P: Protocol + IntoDyn<T>,
            T: ActorType<Channel = dyn DynChannel>,
        {
            <$ty>::from_inner(self.$at.transform_channel(|c| c))
        }
    };
}

macro_rules! channel_methods {
    ($at:ident) => {
        /// Close the [Channel].
        pub fn close(&self) -> bool {
            self.$at.close()
        }

        /// Halt `n` processes of the actor.
        pub fn halt_some(&self, n: u32) {
            self.$at.halt_some(n)
        }

        /// Halt the actor.
        ///
        /// This also closes the Inbox.
        pub fn halt(&self) {
            self.$at.halt()
        }

        /// Get the amount of processes.
        pub fn process_count(&self) -> usize {
            self.$at.process_count()
        }

        /// Get the amount of messages in the channel.
        pub fn msg_count(&self) -> usize {
            self.$at.msg_count()
        }

        /// Get the amount of addresses of the actor.
        pub fn address_count(&self) -> usize {
            self.$at.address_count()
        }

        /// Whether the channel is closed.
        pub fn is_closed(&self) -> bool {
            self.$at.is_closed()
        }

        /// Get the capacity of the channel.
        pub fn capacity(&self) -> &tiny_actor::Capacity {
            self.$at.capacity()
        }

        /// Whether the actor has exited.
        pub fn has_exited(&self) -> bool {
            self.$at.has_exited()
        }

        /// Get the actor id.
        pub fn actor_id(&self) -> u64 {
            self.$at.actor_id()
        }

        /// Whether the channel is bounded.
        pub fn is_bounded(&self) -> bool {
            self.$at.is_bounded()
        }
    };
}

macro_rules! child_methods {
    ($at:ident) => {
        /// Attach the actor.
        ///
        /// Returns the old abort-timeout if it was already attached.
        pub fn attach(&mut self, duration: Duration) -> Option<Duration> {
            self.$at.attach(duration)
        }

        /// Detach the actor.
        ///
        /// Returns the old abort-timeout if it was attached before.
        pub fn detach(&mut self) -> Option<Duration> {
            self.$at.detach()
        }

        /// Whether the actor has been aborted.
        pub fn is_aborted(&self) -> bool {
            self.$at.is_aborted()
        }

        /// Whether the actor is attached.
        pub fn is_attached(&self) -> bool {
            self.$at.is_attached()
        }

        /// Get the link of the actor.
        pub fn link(&self) -> &tiny_actor::Link {
            self.$at.link()
        }

        /// Abort the actor.
        pub fn abort(&mut self) -> bool {
            self.$at.abort()
        }

        /// Whether the underlying task(s) have finished.
        pub fn is_finished(&self) -> bool {
            self.$at.is_finished()
        }

        /// Get a new address to the actor.
        pub fn get_address(&self) -> Address<T> {
            Address::from_inner(self.inner.get_address())
        }
    };
}

pub(crate) use channel_methods;
pub(crate) use child_methods;
pub(crate) use into_dyn_methods;
pub(crate) use transform_methods;
pub(crate) use unchecked_send_methods;

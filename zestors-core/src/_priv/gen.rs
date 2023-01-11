/// Generates all methods related to the actor, like closing and halting the actor.
macro_rules! channel_methods {
    () => {
        /// Closes the [Inbox] of the actor. The actor may still continue receiving any messages
        /// already in the inbox, but won't be able to receive any new messages.
        pub fn close(&self) -> bool {
            self.channel.close()
        }

        /// Halts n processes of the actor.
        pub fn halt_some(&self, n: u32) {
            self.channel.halt_some(n)
        }

        /// Halts all processes of the actor and also closes the [Inbox].
        pub fn halt(&self) {
            self.channel.halt()
        }

        /// Returns the amount of processes that this actor consists of.
        pub fn process_count(&self) -> usize {
            self.channel.process_count()
        }

        /// Returns the amount of messages currently in the [Inbox].
        pub fn msg_count(&self) -> usize {
            self.channel.msg_count()
        }

        /// Returns the amount of [Addresses](Address) that currently exist of the actor. This does not
        /// count the [Child] or [ChildPool].
        pub fn address_count(&self) -> usize {
            self.channel.address_count()
        }

        /// Whether the [Channel] is closed.
        pub fn is_closed(&self) -> bool {
            self.channel.is_closed()
        }

        pub fn is_bounded(&self) -> bool {
            self.channel.capacity().is_bounded()
        }

        /// Get the [Capacity] of the [Channel].
        pub fn capacity(&self) -> &Capacity {
            self.channel.capacity()
        }

        /// Whether all processes have exited.
        pub fn has_exited(&self) -> bool {
            self.channel.has_exited()
        }

        /// Get the actor's id.
        pub fn actor_id(&self) -> ActorId {
            self.channel.actor_id()
        }
    };
}
pub(crate) use channel_methods;

macro_rules! send_methods {
    ($ty:ty) => {
        pub fn try_send<M>(&self, msg: M) -> Result<Returned<M>, TrySendError<M>>
        where
            M: Message,
            $ty: Accepts<M>,
        {
            <$ty as Accepts<M>>::try_send(&self.channel, msg)
        }

        /// Attempts to send a message to an actor. If the mailbox is full this will fail, but if
        /// a timeout is returned then this will succeed.
        pub fn send_now<M>(&self, msg: M) -> Result<Returned<M>, TrySendError<M>>
        where
            M: Message,
            $ty: Accepts<M>,
        {
            <$ty as Accepts<M>>::send_now(&self.channel, msg)
        }

        /// Same as `send`, but blocks the thread instead.
        pub fn send_blocking<M>(&self, msg: M) -> Result<Returned<M>, SendError<M>>
        where
            M: Message,
            $ty: Accepts<M>,
        {
            <$ty as Accepts<M>>::send_blocking(&self.channel, msg)
        }

        /// Send a message to the actor. If the inbox is full or returns a timeout, wait for this
        /// and then send it.
        pub fn send<M>(&self, msg: M) -> <$ty>::SendFut<'_>
        where
            M: Message,
            $ty: Accepts<M>,
        {
            <$ty as Accepts<M>>::send(&self.channel, msg)
        }
    };
}
pub(crate) use send_methods;

macro_rules! unchecked_send_methods {
    () => {
        /// Send a message to an actor without checking whether this actor accepts the message.
        ///
        /// If the actor does not accept the message, then nothing is sent and an error is returned.
        pub fn try_send_unchecked<M>(&self, msg: M) -> Result<Returned<M>, TrySendUncheckedError<M>>
        where
            M: Message + Send + 'static,
            Sent<M>: Send + 'static,
        {
            self.channel.try_send_unchecked(msg)
        }

        /// Send a message to an actor without checking whether this actor accepts the message.
        ///
        /// If the actor does not accept the message, then nothing is sent and an error is returned.
        pub fn send_now_unchecked<M>(&self, msg: M) -> Result<Returned<M>, TrySendUncheckedError<M>>
        where
            M: Message + Send + 'static,
            Sent<M>: Send + 'static,
        {
            self.channel.send_now_unchecked(msg)
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
            self.channel.send_blocking_unchecked(msg)
        }

        /// Send a message to an actor without checking whether this actor accepts the message.
        ///
        /// If the actor does not accept the message, then nothing is sent and an error is returned.
        pub async fn send_unchecked<M>(&self, msg: M) -> Result<Returned<M>, SendUncheckedError<M>>
        where
            M: Message + Send + 'static,
            Sent<M>: Send + 'static,
        {
            self.channel.send_unchecked(msg).await
        }
    };
}

pub(crate) use unchecked_send_methods;

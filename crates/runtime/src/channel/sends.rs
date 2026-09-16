use super::*;

/// Provides message-sending operations for a channel.
///
/// `Sends` exposes four levels of delivery semantics:
///
/// - [`Sends::send`] applies backpressure and asynchronously waits when the
///   channel is under load.
/// - [`Sends::try_send`] applies backpressure but never waits, returning
///   [`ClosedOrFull::Full`] when the channel is under backpressure.
/// - [`Sends::send_now`] checks whether the channel is open but ignores
///   backpressure.
/// - [`Sends::force_send`] ignores both backpressure and the channel status.
pub trait Cast<M: Message>: Sync {
    /// Sends a message, applying backpressure when the channel is under load.
    ///
    /// This method waits asynchronously while backpressure is active. It
    /// returns [`Closed`] if the channel is closed.
    ///
    /// Unlike [`Sends::try_send`], this method waits rather than returning
    /// immediately when backpressure is active.
    fn cast(&self, msg: M) -> impl Future<Output = Result<M::Receipt, CastError<M>>> + Send;

    /// Attempts to send a message without waiting.
    ///
    /// Returns [`ClosedOrFull::Full`] if the channel has reached its
    /// backpressure limit, or [`ClosedOrFull::Closed`] if the channel is
    /// closed.
    ///
    /// Unlike [`Sends::send`], this method never waits for backpressure to
    /// subside.
    fn try_cast(&self, msg: M) -> Result<M::Receipt, TryCastError<M>>;

    /// Sends a message immediately if the channel is open.
    ///
    /// This method ignores backpressure, but still checks whether the channel
    /// is accepting messages. It returns [`Closed`] if the channel is closed.
    ///
    /// Use [`Sends::force_send`] when the channel status should also be
    /// ignored.
    fn cast_now(&self, msg: M) -> Result<M::Receipt, CastError<M>>;

    // /// Sends a message immediately, ignoring backpressure and channel status.
    // ///
    // /// This is the lowest-level sending operation. The message is queued even
    // /// when the channel is closed.
    // ///
    // /// If the underlying queue is at capacity, the message is dropped and the
    // /// implementation may log the overflow.
    // fn force_send(&self, msg: M) -> M::Receipt;

    /// Sends a message and waits for a reply.
    ///
    /// This is the same as [`Sends::send`] with [`MessageOutput::receive`] called on the result. The resulting value is therefore [`Message::Output`] instead of
    /// [`Message::Output`].
    fn call(&self, msg: M) -> impl Future<Output = Result<M::Output, CallError<M>>> + Send {
        async move { Ok(self.cast(msg).await?.wait().await?) }
    }
}

impl<M, H> Cast<M> for H
where
    H: ActorRef + Sync,
    M: Message,
    Channel<H::Ctx>: _Cast<M>,
{
    async fn cast(&self, msg: M) -> Result<M::Receipt, CastError<M>> {
        self.channel()._cast(msg).await
    }

    fn try_cast(&self, msg: M) -> Result<M::Receipt, TryCastError<M>> {
        self.channel()._try_cast(msg)
    }

    fn cast_now(&self, msg: M) -> Result<M::Receipt, CastError<M>> {
        self.channel()._cast_now(msg)
    }
}

/// A private trait for implementation on [`ActorHandle`] only.
///
/// There is a blacket-implementation of [`Sends`] for all types that implement
/// [`ActorHandle`].
pub(crate) trait _Cast<M: Message>: Sync {
    fn _cast(&self, msg: M) -> impl Future<Output = Result<M::Receipt, CastError<M>>> + Send;
    fn _try_cast(&self, msg: M) -> Result<M::Receipt, TryCastError<M>>;
    fn _cast_now(&self, msg: M) -> Result<M::Receipt, CastError<M>>;
    fn _call(&self, msg: M) -> impl Future<Output = Result<M::Output, CallError<M>>> + Send {
        async move { Ok(self._cast(msg).await?.wait().await?) }
    }
}

impl<M, I> _Cast<M> for Channel<I>
where
    M: Message,
    I: Interface + TryInto<Envelope<M>> + From<Envelope<M>> + Send + 'static,
{
    async fn _cast(&self, msg: M) -> Result<M::Receipt, CastError<M>> {
        self.delay_for_backpressure().await;
        self._cast_now(msg)
    }

    fn _try_cast(&self, msg: M) -> Result<M::Receipt, TryCastError<M>> {
        if self.reached_backpressure() {
            return Err(TryCastError::Full(msg));
        }

        self._cast_now(msg).map_err(Into::into)
    }

    fn _cast_now(&self, msg: M) -> Result<M::Receipt, CastError<M>> {
        if !self.status().accepts_messages() {
            return Err(CastError(msg));
        }

        if let Some(queue) = self.raw_queue() {
            let (envelope, receipt) = Envelope::new_pair(msg);
            let interface = I::from(envelope);

            if let Err(_e) = queue.push(interface) {
                panic!("Queue was full or empty {}", std::any::type_name::<Self>());
            }

            Ok(receipt)
        } else {
            match self.call_now_dyn(msg) {
                Err(CastCheckedError::NotAccepted(_)) => {
                    panic!(
                        "Message type {} not accepted by channel {}",
                        std::any::type_name::<M>(),
                        std::any::type_name::<Self>(),
                    );
                }
                Err(CastCheckedError::Closed(msg)) => Err(CastError(msg)),
                Ok(output) => Ok(output),
            }
        }
    }
}

impl<M, T> _Cast<M> for Channel<Dyn<T>>
where
    M: Message,
    T: AsTypeSet + Contains<M> + 'static,
{
    async fn _cast(&self, msg: M) -> Result<M::Receipt, CastError<M>> {
        match self.cast_dyn(msg).await {
            Ok(output) => Ok(output),
            Err(CastCheckedError::Closed(msg)) => Err(CastError(msg)),
            Err(CastCheckedError::NotAccepted(_)) => {
                panic!(
                    "Message type {} not accepted by channel {}",
                    std::any::type_name::<M>(),
                    std::any::type_name::<Self>(),
                );
            }
        }
    }

    fn _try_cast(&self, msg: M) -> Result<M::Receipt, TryCastError<M>> {
        match self.try_cast_dyn(msg) {
            Ok(output) => Ok(output),
            Err(TryCastCheckedError::Closed(msg)) => Err(TryCastError::Closed(msg)),
            Err(TryCastCheckedError::Full(msg)) => Err(TryCastError::Full(msg)),
            Err(TryCastCheckedError::NotAccepted(_)) => {
                panic!(
                    "Message type {} not accepted by channel {}",
                    std::any::type_name::<M>(),
                    std::any::type_name::<Self>(),
                );
            }
        }
    }

    fn _cast_now(&self, msg: M) -> Result<M::Receipt, CastError<M>> {
        match self.call_now_dyn(msg) {
            Ok(output) => Ok(output),
            Err(CastCheckedError::Closed(msg)) => Err(CastError(msg)),
            Err(CastCheckedError::NotAccepted(_)) => {
                panic!(
                    "Message type {} not accepted by channel {}",
                    std::any::type_name::<M>(),
                    std::any::type_name::<Self>(),
                );
            }
        }
    }
}

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
    fn cast(&self, msg: M) -> impl Future<Output = Result<M::Receipt, CastError<M>>> + Send {
        self.cast_with(msg, Default::default())
    }

    fn cast_with(
        &self,
        msg: M,
        options: CastOptions,
    ) -> impl Future<Output = Result<M::Receipt, CastError<M>>> + Send;

    /// Sends a message immediately if the channel is open.
    ///
    /// This method ignores backpressure, but still checks whether the channel
    /// is accepting messages. It returns [`Closed`] if the channel is closed.
    ///
    /// Use [`Sends::force_send`] when the channel status should also be
    /// ignored.
    fn cast_now(&self, msg: M) -> Result<M::Receipt, CastNowError<M>> {
        self.cast_now_with(msg, Default::default())
    }

    fn cast_now_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, CastNowError<M>>;

    // fn cast_now_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, CastError<M>>;

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

    fn call_with(
        &self,
        msg: M,
        options: CastOptions,
    ) -> impl Future<Output = Result<M::Output, CallError<M>>> + Send {
        async move { Ok(self.cast_with(msg, options).await?.wait().await?) }
    }
}

impl<M, H> Cast<M> for H
where
    H: ActorRef + Sync,
    M: Message,
    Channel<H::Ctx>: _Cast<M>,
{
    async fn cast_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, CastError<M>> {
        self.channel()._cast_with(msg, options).await
    }

    fn cast_now(&self, msg: M) -> Result<M::Receipt, CastNowError<M>> {
        self.cast_now_with(msg, Default::default())
    }

    fn cast_now_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, CastNowError<M>> {
        self.channel()._cast_now_with(msg, options)
    }
}

/// A private trait for implementation on [`ActorHandle`] only.
///
/// There is a blacket-implementation of [`Sends`] for all types that implement
/// [`ActorHandle`].
pub(crate) trait _Cast<M: Message>: Sync {
    fn _cast_with(
        &self,
        msg: M,
        options: CastOptions,
    ) -> impl Future<Output = Result<M::Receipt, CastError<M>>> + Send;

    fn _cast_now_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, CastNowError<M>>;

    fn _call_with(
        &self,
        msg: M,
        options: CastOptions,
    ) -> impl Future<Output = Result<M::Output, CallError<M>>> + Send {
        async move { Ok(self._cast_with(msg, options).await?.wait().await?) }
    }
}

impl<M, I> _Cast<M> for Channel<I>
where
    M: Message,
    I: Interface + TryInto<Envelope<M>> + From<Envelope<M>> + Send + 'static,
{
    async fn _cast_with(
        &self,
        msg: M,
        mut options: CastOptions,
    ) -> Result<M::Receipt, CastError<M>> {
        if !options.ignore_backpressure {
            self.delay_for_backpressure().await;
            options.ignore_backpressure = true;
        }

        self._cast_now_with(msg, options)
            .map_err(|e| e.into_cast_error_dbg_assert())
    }

    fn _cast_now_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, CastNowError<M>> {
        let status = self.status();
        if !status.accepts_messages() && !(options.ignore_exiting && status.is_shutting_down()) {
            return Err(CastNowError::Closed(msg));
        }

        if let Some(queue) = self.raw_queue() {
            let (envelope, receipt) = Envelope::new_pair(msg);
            let interface = I::from(envelope);

            if let Err(_e) = queue.push(interface) {
                panic!("Queue was full or empty {}", std::any::type_name::<Self>());
            }

            Ok(receipt)
        } else {
            match self.cast_now_dyn_with(msg, options) {
                Err(CastNowDynError::NotAccepted(_)) => {
                    panic!(
                        "Message type {} not accepted by channel {}",
                        std::any::type_name::<M>(),
                        std::any::type_name::<Self>(),
                    );
                }
                Err(CastNowDynError::Closed(msg)) => Err(CastNowError::Closed(msg)),
                Err(CastNowDynError::Full(msg)) => Err(CastNowError::Full(msg)),
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
    async fn _cast_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, CastError<M>> {
        match self.cast_dyn_with(msg, options).await {
            Ok(output) => Ok(output),
            Err(CastDynError::Closed(msg)) => Err(CastError(msg)),
            Err(CastDynError::NotAccepted(_)) => {
                panic!(
                    "Message type {} not accepted by channel {}",
                    std::any::type_name::<M>(),
                    std::any::type_name::<Self>(),
                );
            }
        }
    }

    fn _cast_now_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, CastNowError<M>> {
        match self.cast_now_dyn_with(msg, options) {
            Ok(output) => Ok(output),
            Err(CastNowDynError::Closed(msg)) => Err(CastNowError::Closed(msg)),
            Err(CastNowDynError::Full(msg)) => Err(CastNowError::Full(msg)),
            Err(CastNowDynError::NotAccepted(_)) => {
                panic!(
                    "Message type {} not accepted by channel {}",
                    std::any::type_name::<M>(),
                    std::any::type_name::<Self>(),
                );
            }
        }
    }
}

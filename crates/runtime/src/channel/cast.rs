use super::*;

impl Channel {
    // Statically sends a message
    pub(crate) async fn cast_with<M, I>(
        &self,
        msg: M,
        mut options: CallOptions,
    ) -> Result<ReceiptOf<M>, CastError<M>>
    where
        M: Message,
        I: Interface + TryInto<Envelope<M>> + From<Envelope<M>> + Send + 'static,
    {
        if !options.ignore_backpressure {
            self.delay_for_backpressure().await;
            options.ignore_backpressure = true;
        }

        self.try_cast_with::<M, I>(msg, options)
            .map_err(|e| e.into_cast_error_dbg_assert())
    }

    // Dynamically sends a message
    pub(crate) fn cast_dyn_with<M: Message>(
        &self,
        msg: M,
        mut options: CallOptions,
    ) -> impl Future<Output = Result<ReceiptOf<M>, CastDynError<M>>> + Send {
        async move {
            if !options.ignore_backpressure {
                self.delay_for_backpressure().await;
                options.ignore_backpressure = true;
            }

            self.try_cast_dyn_with(msg, options)
                .map_err(|e| e.into_cast_error_dbg_assert())
        }
    }

    /// Tries to statically send a message through the channel, not waiting for space
    /// to become available in the channel's queue.
    ///
    /// This method falls back to the dynamic implementation if the static queue for the message type is not available.
    pub(crate) fn try_cast_with<M, I>(
        &self,
        msg: M,
        options: CallOptions,
    ) -> Result<ReceiptOf<M>, TryCastError<M>>
    where
        M: Message,
        I: Interface + TryInto<Envelope<M>> + From<Envelope<M>> + Send + 'static,
    {
        // Fallback to dynamic implementation
        let Some(queue) = self.raw_queue::<I>() else {
            return match self.try_cast_dyn_with(msg, options) {
                Ok(output) => Ok(output),
                Err(TryCastDynError::Closed(msg)) => Err(TryCastError::Closed(msg)),
                Err(TryCastDynError::Full(msg)) => Err(TryCastError::Full(msg)),
                Err(TryCastDynError::NotAccepted(_)) => {
                    panic!(
                        "Message type {} not accepted by channel {}",
                        std::any::type_name::<M>(),
                        std::any::type_name::<Self>(),
                    );
                }
            };
        };

        if !options.ignore_backpressure && self.reached_backpressure() {
            return Err(TryCastError::Full(msg));
        }

        let status = self.status();
        if !status.accepts_messages() && !(options.ignore_exiting && status.is_exiting()) {
            return Err(TryCastError::Closed(msg));
        }

        let (envelope, receipt) = Envelope::new_pair(msg);
        let interface = I::from(envelope);

        if let Err(_e) = queue.push(interface) {
            panic!("Queue was full or empty {}", std::any::type_name::<Self>());
        }

        self.msg_notify_one();
        Ok(receipt)
    }

    /// Tries to dynamically send a message through the channel, not waiting for space
    /// to become available in the channel's queue.
    pub(crate) fn try_cast_dyn_with<M: Message>(
        &self,
        msg: M,
        options: CallOptions,
    ) -> Result<ReceiptOf<M>, TryCastDynError<M>> {
        if !options.ignore_backpressure && self.reached_backpressure() {
            return Err(TryCastDynError::Full(msg));
        }

        let status = self.status();
        if !status.accepts_messages() && !(options.ignore_exiting && status.is_exiting()) {
            return Err(TryCastDynError::Closed(msg));
        }

        let output = self.try_push_msg(msg)?;
        self.msg_notify_one();
        Ok(output)
    }
}

/// A private trait for implementation on [`Address`] only.
///
/// There is a blanket implementation of [`Accepts`] for all types that implement
/// [`ActorRef`], provided their [`Address`] implements this trait.
pub(crate) trait Casts<M: Message>: Sync {
    /// The [`Address`]-specific implementation backing [`Accepts::cast_with`].
    fn _cast_with(
        &self,
        msg: M,
        options: CallOptions,
    ) -> impl Future<Output = Result<ReceiptOf<M>, CastError<M>>> + Send;

    /// The [`Address`]-specific implementation backing [`Accepts::try_cast_with`].
    fn _try_cast_with(&self, msg: M, options: CallOptions)
    -> Result<ReceiptOf<M>, TryCastError<M>>;
}

impl<M, I> Casts<M> for Address<I>
where
    M: Message,
    I: Interface + TryInto<Envelope<M>> + From<Envelope<M>> + Send + 'static,
{
    async fn _cast_with(&self, msg: M, options: CallOptions) -> Result<ReceiptOf<M>, CastError<M>> {
        self._channel().cast_with::<M, I>(msg, options).await
    }

    fn _try_cast_with(
        &self,
        msg: M,
        options: CallOptions,
    ) -> Result<ReceiptOf<M>, TryCastError<M>> {
        self._channel().try_cast_with::<M, I>(msg, options)
    }
}

impl<M, T> Casts<M> for Address<Dyn<T>>
where
    M: Message,
    T: AsTypeSet + Contains<M> + 'static,
{
    async fn _cast_with(&self, msg: M, options: CallOptions) -> Result<ReceiptOf<M>, CastError<M>> {
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

    fn _try_cast_with(
        &self,
        msg: M,
        options: CallOptions,
    ) -> Result<ReceiptOf<M>, TryCastError<M>> {
        match self.try_cast_dyn_with(msg, options) {
            Ok(output) => Ok(output),
            Err(TryCastDynError::Closed(msg)) => Err(TryCastError::Closed(msg)),
            Err(TryCastDynError::Full(msg)) => Err(TryCastError::Full(msg)),
            Err(TryCastDynError::NotAccepted(_)) => {
                panic!(
                    "Message type {} not accepted by channel {}",
                    std::any::type_name::<M>(),
                    std::any::type_name::<Self>(),
                );
            }
        }
    }
}

use super::*;

/// Provides message-sending operations for a channel.
///
/// `Cast` exposes two ways to send a message, both configurable through a
/// shared [`CastOptions`]:
///
/// - [`Cast::cast`] / [`Cast::cast_with`] wait out backpressure before
///   sending, and so only fail if the channel is closed.
/// - [`Cast::try_cast`] / [`Cast::try_cast_with`] never wait: they fail
///   immediately if the channel is closed, and may also fail if it is
///   currently under backpressure (see [`Cast::try_cast_with`]).
///
/// [`Cast::call`] / [`Cast::call_with`] build on `cast`/`cast_with` to
/// additionally wait for the message's reply.
pub trait Cast<M: Message>: Sync {
    /// Sends a message, waiting out backpressure first if the channel is
    /// under load.
    ///
    /// Equivalent to [`Cast::cast_with`] with the default [`CastOptions`].
    fn cast(&self, msg: M) -> impl Future<Output = Result<M::Receipt, CastError<M>>> + Send {
        self.cast_with(msg, Default::default())
    }

    /// Same as [`Cast::cast`], with explicit [`CastOptions`].
    ///
    /// Unless `options.ignore_backpressure` is `true`, this first waits for as
    /// long as the channel's current backpressure delay dictates (based on how
    /// full the channel is), then sends regardless of backpressure. Because of
    /// this, `cast_with` never fails due to the channel being full — its only
    /// failure mode is [`CastError`], returned if the channel is closed (which,
    /// unless `options.ignore_exiting` is `true`, includes a channel that is
    /// [`ActorStatus::Exiting`]).
    fn cast_with(
        &self,
        msg: M,
        options: CastOptions,
    ) -> impl Future<Output = Result<M::Receipt, CastError<M>>> + Send;

    /// Sends a message immediately, without waiting out backpressure.
    ///
    /// Equivalent to [`Cast::try_cast_with`] with the default [`CastOptions`].
    fn try_cast(&self, msg: M) -> Result<M::Receipt, TryCastError<M>> {
        self.try_cast_with(msg, Default::default())
    }

    /// Same as [`Cast::try_cast`], with explicit [`CastOptions`].
    ///
    /// Returns [`TryCastError::Closed`] if the channel is closed (which,
    /// unless `options.ignore_exiting` is `true`, includes a channel that is
    /// [`ActorStatus::Exiting`]).
    ///
    /// Returns [`TryCastError::Full`] if `options.ignore_backpressure` is
    /// `false` and the channel is currently under backpressure.
    fn try_cast_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, TryCastError<M>>;

    /// Sends a message via [`Cast::cast`] and waits for its reply.
    ///
    /// Equivalent to calling [`Cast::cast`] and then [`Receipt::wait`] on the
    /// result, so it shares `cast`'s backpressure and closed-channel behavior.
    /// The output is therefore [`Message::Output`] (the reply) rather than
    /// [`Message::Receipt`] (the handle used to await it).
    ///
    /// Returns [`CallError::Closed`] if the channel was closed at the time of
    /// sending, or [`CallError::NoResponse`] if no reply was ever received
    /// (for example, because the actor exited, or dropped the request, before
    /// replying).
    fn call(&self, msg: M) -> impl Future<Output = Result<M::Output, CallError<M>>> + Send {
        async move { Ok(self.cast(msg).await?.wait().await?) }
    }

    /// Same as [`Cast::call`], with explicit [`CastOptions`] applied to the
    /// underlying [`Cast::cast_with`] call.
    fn call_with(
        &self,
        msg: M,
        options: CastOptions,
    ) -> impl Future<Output = Result<M::Output, CallError<M>>> + Send {
        async move { Ok(self.cast_with(msg, options).await?.wait().await?) }
    }
}

impl<M, T> Cast<M> for T
where
    T: ActorRef + Sync,
    M: Message,
    Channel<T::Ctx>: _Cast<M>,
{
    async fn cast_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, CastError<M>> {
        self.channel()._cast_with(msg, options).await
    }

    fn try_cast(&self, msg: M) -> Result<M::Receipt, TryCastError<M>> {
        self.try_cast_with(msg, Default::default())
    }

    fn try_cast_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, TryCastError<M>> {
        self.channel()._try_cast_with(msg, options)
    }
}

/// A private trait for implementation on [`Channel`] only.
///
/// There is a blanket implementation of [`Cast`] for all types that implement
/// [`ActorRef`], provided their [`Channel`] implements this trait.
pub(crate) trait _Cast<M: Message>: Sync {
    /// The [`Channel`]-specific implementation backing [`Cast::cast_with`].
    fn _cast_with(
        &self,
        msg: M,
        options: CastOptions,
    ) -> impl Future<Output = Result<M::Receipt, CastError<M>>> + Send;

    /// The [`Channel`]-specific implementation backing [`Cast::try_cast_with`].
    fn _try_cast_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, TryCastError<M>>;

    /// The [`Channel`]-specific implementation backing [`Cast::call_with`].
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

        self._try_cast_with(msg, options)
            .map_err(|e| e.into_cast_error_dbg_assert())
    }

    fn _try_cast_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, TryCastError<M>> {
        if let Some(queue) = self.raw_queue() {
            if !options.ignore_backpressure && self.reached_backpressure() {
                return Err(TryCastError::Full(msg));
            }

            let status = self.status();
            if !status.accepts_messages() && !(options.ignore_exiting && status.is_shutting_down())
            {
                return Err(TryCastError::Closed(msg));
            }

            let (envelope, receipt) = Envelope::new_pair(msg);
            let interface = I::from(envelope);

            if let Err(_e) = queue.push(interface) {
                panic!("Queue was full or empty {}", std::any::type_name::<Self>());
            }

            Ok(receipt)
        } else {
            match self.try_cast_dyn_with(msg, options) {
                Err(TryCastDynError::NotAccepted(_)) => {
                    panic!(
                        "Message type {} not accepted by channel {}",
                        std::any::type_name::<M>(),
                        std::any::type_name::<Self>(),
                    );
                }
                Err(TryCastDynError::Closed(msg)) => Err(TryCastError::Closed(msg)),
                Err(TryCastDynError::Full(msg)) => Err(TryCastError::Full(msg)),
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

    fn _try_cast_with(&self, msg: M, options: CastOptions) -> Result<M::Receipt, TryCastError<M>> {
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

use super::*;

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
    ) -> impl Future<Output = Result<M::Receipt, CastError<M>>> + Send;

    /// The [`Address`]-specific implementation backing [`Accepts::try_cast_with`].
    fn _try_cast_with(&self, msg: M, options: CallOptions) -> Result<M::Receipt, TryCastError<M>>;
}

impl<M, I> Casts<M> for Address<I>
where
    M: Message,
    I: Interface + TryInto<Envelope<M>> + From<Envelope<M>> + Send + 'static,
{
    async fn _cast_with(&self, msg: M, options: CallOptions) -> Result<M::Receipt, CastError<M>> {
        self._channel().cast_with::<M, I>(msg, options).await
    }

    fn _try_cast_with(&self, msg: M, options: CallOptions) -> Result<M::Receipt, TryCastError<M>> {
        self._channel().try_cast_with::<M, I>(msg, options)
    }
}

impl<M, T> Casts<M> for Address<Dyn<T>>
where
    M: Message,
    T: AsTypeSet + Contains<M> + 'static,
{
    async fn _cast_with(&self, msg: M, options: CallOptions) -> Result<M::Receipt, CastError<M>> {
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

    fn _try_cast_with(&self, msg: M, options: CallOptions) -> Result<M::Receipt, TryCastError<M>> {
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

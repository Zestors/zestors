use crate::{
    protocol::{Message, Returned},
    *,
};
use futures::{Future, FutureExt};
use std::pin::Pin;
use tiny_actor::SendError;

/// Future returned when sending a message to an [Address].
pub struct SendFut<'a, M: Message>(
    pub(crate) Pin<Box<dyn Future<Output = Result<Returned<M>, SendError<M>>> + Send + 'a>>,
);

impl<'a, M: Message> Unpin for SendFut<'a, M> {}

impl<'a, M: Message> Future for SendFut<'a, M> {
    type Output = Result<Returned<M>, SendError<M>>;

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.0.poll_unpin(cx)
    }
}

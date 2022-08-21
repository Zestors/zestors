use crate::*;
use futures::{Future, FutureExt};
use std::pin::Pin;
use tiny_actor::SendError;

pub struct SendFut<'a, M: Message>(
    pub(crate) Pin<Box<dyn Future<Output = Result<Returns<M>, SendError<M>>> + Send + 'a>>,
);

impl<'a, M: Message> Unpin for SendFut<'a, M> {}

impl<'a, M: Message> Future for SendFut<'a, M> {
    type Output = Result<Returns<M>, SendError<M>>;

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.0.poll_unpin(cx)
    }
}

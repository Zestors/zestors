use crate::{_gen, channel::ActorType, protocol::Protocol};

use crate::*;
use futures::{stream::FusedStream, Stream, StreamExt};
use tiny_actor::{HaltedError, TryRecvError};

#[derive(Debug)]
pub struct Inbox<P> {
    inbox: tiny_actor::Inbox<P>,
}

impl<T> Inbox<T> {
    pub(crate) fn from_inner(inbox: tiny_actor::Inbox<T>) -> Self {
        Self { inbox }
    }

    /// Attempt to receive a message from the [Inbox]. If there is no message, this
    /// returns `None`.
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        self.inbox.try_recv()
    }

    /// Wait until there is a message in the [Inbox], or until the channel is closed.
    pub fn recv(&mut self) -> RecvFut<'_, T> {
        self.inbox.recv()
    }

    pub fn get_address(&self) -> Address<T>
    where
        T: Protocol,
    {
        let address = self.inbox.get_address();
        Address::from_inner(address)
    }

    _gen::channel_methods!(inbox);
}

impl<T> Inbox<T>
where
    T: ActorType<Channel = Channel<T>>,
{
    _gen::send_methods!(inbox);
}

impl<T> Stream for Inbox<T> {
    type Item = Result<T, HaltedError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inbox.poll_next_unpin(cx)
    }
}

impl<M> FusedStream for Inbox<M> {
    fn is_terminated(&self) -> bool {
        self.inbox.is_terminated()
    }
}

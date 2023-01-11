use event_listener::EventListener;
use futures::{stream::FusedStream, FutureExt, Stream};

use crate::*;
use std::{fmt::Debug, sync::Arc};

/// An Inbox is a non clone-able receiver part of a channel.
///
/// An Inbox is mostly used to receive messages, with [Inbox::recv], [Inbox::try_recv] or
/// [futures::Stream].
#[derive(Debug)]
pub struct Inbox<P> {
    // The underlying channel
    channel: Arc<InboxChannel<P>>,
    // Whether the inbox has signaled halt yet
    signaled_halt: bool,
    // The recv_listener for streams and Rcv
    recv_listener: Option<EventListener>,
}

impl<P> Inbox<P> {
    /// This does not increment the inbox_count.
    pub(crate) fn new(channel: Arc<InboxChannel<P>>) -> Self {
        Inbox {
            channel,
            signaled_halt: false,
            recv_listener: None,
        }
    }

    pub(crate) fn channel_ref(&self) -> &Arc<InboxChannel<P>> {
        &self.channel
    }

    /// Attempt to receive a message from the [Inbox]. If there is no message, this
    /// returns `None`.
    pub fn try_recv(&mut self) -> Result<P, TryRecvError> {
        self.channel.try_recv(&mut self.signaled_halt)
    }

    /// Wait until there is a message in the [Inbox], or until the channel is closed.
    pub fn recv(&mut self) -> RecvFut<'_, P> {
        self.channel
            .recv(&mut self.signaled_halt, &mut self.recv_listener)
    }

    /// Get a new [Address] to the [Channel].
    pub fn get_address(&self) -> Address<P>
    where
        P: Protocol,
    {
        self.channel.add_address();
        Address::new(self.channel.clone())
    }

    gen::channel_methods!();
}

impl<P> Inbox<P>
where
    P: DefinesChannel<Channel = InboxChannel<P>>,
{
    gen::send_methods!(P);
}

impl<P: Protocol> SpawnsWith for Inbox<P> {
    type ChannelDefinition = P;
    type Config = Config;

    fn setup_channel(
        config: Config,
        inbox_count: usize,
        address_count: usize
    ) -> (Arc<<Self::ChannelDefinition as DefinesChannel>::Channel>, Link) {
        (
            Arc::new(InboxChannel::new(address_count, inbox_count, config.capacity)),
            config.link,
        )
    }

    fn new(channel: Arc<<Self::ChannelDefinition as DefinesChannel>::Channel>) -> Self {
        Self::new(channel)
    }
}

impl<P> Stream for Inbox<P> {
    type Item = Result<P, HaltedError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.recv().poll_unpin(cx).map(|res| match res {
            Ok(msg) => Some(Ok(msg)),
            Err(e) => match e {
                RecvError::Halted => Some(Err(HaltedError)),
                RecvError::ClosedAndEmpty => None,
            },
        })
    }
}

impl<P> FusedStream for Inbox<P> {
    fn is_terminated(&self) -> bool {
        self.channel.is_closed()
    }
}

impl<P> Drop for Inbox<P> {
    fn drop(&mut self) {
        self.channel.remove_inbox();
    }
}

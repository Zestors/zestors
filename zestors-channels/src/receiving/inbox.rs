use zestors_core::*;
use event_listener::EventListener;
use futures::{stream::FusedStream, Future, FutureExt, Stream};
use std::{
    fmt::Debug,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use thiserror::Error;

use crate::inbox_channel::{InboxChannel, RecvRawFut};

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

    /// Attempt to receive a message from the [Inbox]. If there is no message, this
    /// returns `None`.
    pub fn try_recv(&mut self) -> Result<P, TryRecvError> {
        self.channel.try_recv(&mut self.signaled_halt)
    }

    /// Wait until there is a message in the [Inbox], or until the channel is closed.
    pub fn recv(&mut self) -> RecvFut<'_, P> {
        RecvFut(
            self.channel
                .recv(&mut self.signaled_halt, &mut self.recv_listener),
        )
    }

    /// Get a new [Address] to the [Channel].
    pub fn get_address(&self) -> Address<Inbox<P>>
    where
        P: Protocol,
    {
        self.channel.add_address();
        Address::from_channel(self.channel.clone())
    }

    // gen::channel_methods!();
}

impl<P> Inbox<P>
where
    P: DefineChannel<Channel = InboxChannel<P>>,
{
    // gen::send_methods!(P);
}

impl<P: Protocol> ActorRef for Inbox<P> {
    type ChannelDefinition = Inbox<P>;

    fn close(&self) -> bool {
        todo!()
    }

    fn halt_some(&self, n: u32) {
        todo!()
    }

    fn halt(&self) {
        todo!()
    }

    fn process_count(&self) -> usize {
        todo!()
    }

    fn msg_count(&self) -> usize {
        todo!()
    }

    fn address_count(&self) -> usize {
        todo!()
    }

    fn is_closed(&self) -> bool {
        todo!()
    }

    fn is_bounded(&self) -> bool {
        todo!()
    }

    fn capacity(&self) -> &Capacity {
        todo!()
    }

    fn has_exited(&self) -> bool {
        todo!()
    }

    fn actor_id(&self) -> ActorId {
        todo!()
    }

    fn get_address(&self) -> Address<Self::ChannelDefinition> {
        todo!()
    }

    fn try_send<M>(&self, msg: M) -> Result<Returned<M>, TrySendError<M>>
    where
        M: Message,
        Self::ChannelDefinition: Accept<M>,
    {
        todo!()
    }

    fn send_now<M>(&self, msg: M) -> Result<Returned<M>, TrySendError<M>>
    where
        M: Message,
        Self::ChannelDefinition: Accept<M>,
    {
        todo!()
    }

    fn send_blocking<M>(&self, msg: M) -> Result<Returned<M>, SendError<M>>
    where
        M: Message,
        Self::ChannelDefinition: Accept<M>,
    {
        todo!()
    }

    fn send<M>(&self, msg: M) -> <Self::ChannelDefinition as Accept<M>>::SendFut<'_>
    where
        M: Message,
        Self::ChannelDefinition: Accept<M>,
    {
        todo!()
    }
}

impl<P: Protocol + Send> Spawn for Inbox<P> {
    type Config = Config;

    fn setup_channel(
        config: Config,
        inbox_count: usize,
        address_count: usize,
        actor_id: ActorId,
    ) -> (
        Arc<<Self::ChannelDefinition as DefineChannel>::Channel>,
        Link,
    ) {
        (
            Arc::new(InboxChannel::new(
                address_count,
                inbox_count,
                config.capacity,
                actor_id,
            )),
            config.link,
        )
    }

    fn new(channel: Arc<<Self::ChannelDefinition as DefineChannel>::Channel>) -> Self {
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

//------------------------------------------------------------------------------------------------
//  RecvFut
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct RecvFut<'a, P>(RecvRawFut<'a, P>);

impl<'a, M> Unpin for RecvFut<'a, M> {}

impl<'a, M> Future for RecvFut<'a, M> {
    type Output = Result<M, RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll_unpin(cx)
    }
}

//------------------------------------------------------------------------------------------------
//  Errors
//------------------------------------------------------------------------------------------------

/// Error returned when receiving a message from an inbox.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum RecvError {
    /// Process has been halted and should now exit.
    #[error("Couldn't receive because the process has been halted")]
    Halted,
    /// Channel has been closed, and contains no more messages. It is impossible for new
    /// messages to be sent to the channel.
    #[error("Couldn't receive becuase the channel is closed and empty")]
    ClosedAndEmpty,
}

/// Error returned when receiving a message from an inbox.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum TryRecvError {
    /// Process has been halted and should now exit.
    #[error("Couldn't receive because the process has been halted")]
    Halted,
    /// The channel is empty, but is not yet closed. New messges may arrive
    #[error("Couldn't receive because the channel is empty")]
    Empty,
    /// Channel has been closed, and contains no more messages. It is impossible for new
    /// messages to be sent to the channel.
    #[error("Couldn't receive becuase the channel is closed and empty")]
    ClosedAndEmpty,
}

/// Error returned when `Stream`ing an [Inbox].
///
/// Process has been halted and should now exit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
#[error("Process has been halted")]
pub struct HaltedError;

use super::*;
use event_listener::EventListener;
use futures::{future::BoxFuture, stream::FusedStream, FutureExt, Stream};
use std::{fmt::Debug, sync::Arc};
use zestors_core::{
    actor_type::{Accept, ActorType},
    inboxes::{Capacity, InboxType, GroupInboxType},
    messaging::{Message, Protocol, ProtocolFrom, SendError, TrySendError},
    monitoring::{ActorId, Channel},
    spawning::ActorRef,
};

/// An Inbox is a non clone-able receiver part of a channel.
///
/// An Inbox is mostly used to receive messages, with [Inbox::recv], [Inbox::try_recv] or
/// [futures::Stream].
#[derive(Debug)]
pub struct Inbox<P: Protocol> {
    // The underlying channel
    channel: Arc<InboxChannel<P>>,
    // Whether the inbox has signaled halt yet
    signaled_halt: bool,
    // The recv_listener for streams and Rcv
    recv_listener: Option<EventListener>,
}

impl<P: Protocol> Inbox<P> {
    /// This does not increment the inbox_count.
    pub(crate) fn new(channel: Arc<InboxChannel<P>>) -> Self {
        Inbox {
            channel,
            signaled_halt: false,
            recv_listener: None,
        }
    }

    pub fn is_halted(&self) -> bool {
        self.signaled_halt
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
    pub fn channel(&self) -> &Arc<InboxChannel<P>>
    where
        P: Protocol,
    {
        &self.channel
    }
}

impl<P: Protocol> ActorType for Inbox<P> {
    type Channel = InboxChannel<P>;
}

impl<P: Protocol> ActorRef for Inbox<P> {
    type ActorType = Self;

    fn channel(&self) -> &Arc<<Self::ActorType as ActorType>::Channel> {
        &self.channel
    }
}

impl<P: Protocol + Send> GroupInboxType for Inbox<P> {
    fn setup_multi_channel(
        config: Self::Config,
        process_count: usize,
        address_count: usize,
        actor_id: ActorId,
    ) -> Arc<Self::Channel> {
        Arc::new(InboxChannel::new(
            address_count,
            process_count,
            config,
            actor_id,
        ))
    }
}

impl<P: Protocol + Send> InboxType for Inbox<P> {
    type Config = Capacity;

    fn setup_channel(
        config: Capacity,
        address_count: usize,
        actor_id: ActorId,
    ) -> Arc<<Self::ActorType as ActorType>::Channel> {
        Self::setup_multi_channel(config, 1, address_count, actor_id)
    }

    fn from_channel(channel: Arc<<Self::ActorType as ActorType>::Channel>) -> Self {
        Self::new(channel)
    }
}

fn unwrap_then_cancel<P: ProtocolFrom<M>, M: Message>(prot: P, returned: M::Returned) -> M {
    let Ok(sent) = prot.try_into_msg() else {
        panic!("")
    };
    M::cancel(sent, returned)
}

impl<M, P> Accept<M> for Inbox<P>
where
    P: Protocol + ProtocolFrom<M>,
    M: Message + Send + 'static,
    M::Payload: Send,
    M::Returned: Send,
{
    type SendFut<'a> = BoxFuture<'a, Result<M::Returned, SendError<M>>>;

    fn try_send(address: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>> {
        let (sends, returns) = M::create(msg);

        match address.try_send_protocol(P::from_msg(sends)) {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendError::Closed(prot) => {
                    Err(TrySendError::Closed(unwrap_then_cancel(prot, returns)))
                }
                TrySendError::Full(prot) => {
                    Err(TrySendError::Full(unwrap_then_cancel(prot, returns)))
                }
            },
        }
    }

    fn force_send(address: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>> {
        let (sends, returns) = M::create(msg);

        match address.send_protocol_now(P::from_msg(sends)) {
            Ok(()) => Ok(returns),
            Err(e) => match e {
                TrySendError::Closed(prot) => {
                    Err(TrySendError::Closed(unwrap_then_cancel(prot, returns)))
                }
                TrySendError::Full(prot) => {
                    Err(TrySendError::Full(unwrap_then_cancel(prot, returns)))
                }
            },
        }
    }

    fn send_blocking(address: &Self::Channel, msg: M) -> Result<M::Returned, SendError<M>> {
        let (sends, returns) = M::create(msg);

        match address.send_protocol_blocking(P::from_msg(sends)) {
            Ok(()) => Ok(returns),
            Err(SendError(prot)) => Err(SendError(unwrap_then_cancel(prot, returns))),
        }
    }

    fn send(address: &Self::Channel, msg: M) -> Self::SendFut<'_> {
        Box::pin(async move {
            let (sends, returns) = M::create(msg);

            match address.send_protocol(P::from_msg(sends)).await {
                Ok(()) => Ok(returns),
                Err(SendError(prot)) => Err(SendError(unwrap_then_cancel(prot, returns))),
            }
        })
    }
}

impl<P: Protocol> Stream for Inbox<P> {
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

impl<P: Protocol> FusedStream for Inbox<P> {
    fn is_terminated(&self) -> bool {
        self.channel.is_closed()
    }
}

impl<P: Protocol> Drop for Inbox<P> {
    fn drop(&mut self) {
        self.channel.remove_inbox();
    }
}

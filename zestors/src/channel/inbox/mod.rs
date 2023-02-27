/*!
# Inbox type
Actors can be spawned using anything that implements [`InboxType`], by default this is
implemented for an [`inbox`] and a [`halter`]. Every inbox-type has once associated type,
[`InboxType::Config`], which is the configuration that the inbox is spawned with. An
inbox-type must also implement [`ActorType`], which means that the [`Address<A>`] and
[`Child<A>`] will be typed with `A` equal to the inbox-type the actor was spawned with.

# Capacity
The standard configuration for inboxes that receive messages is a [`Capacity`]. This
type specifies whether the inbox is bounded or unbounded. If it is bounded then a size
is specified, and if it is unbounded then a [`BackPressure`] must be given.

# Back pressure
If the inbox is unbounded, it has a [`BackPressure`] which defines how a message-overflow
should be handled. By default the backpressure is [`expenential`](BackPressure::exponential)
with the following parameters:
- `starts_at: 5` - The backpressure mechanism should start if the inbox contains 5 or more
messages.
- `timeout: 25ns` - The timeout at which the backpressure mechanism starts is 25ns.
- `factor: 1.3` - For every message in the inbox, the timeout is multiplied by 1.3.

The backpressure can also be set to [`linear`](BackPressure::linear) or
[`disabled`](BackPressure::disabled).

| __<--__ [`spawning`] | [`supervision`] __-->__ |
|---|---|
 */

mod channel;
mod errors;
pub use channel::*;
pub use errors::*;
mod config;
pub use config::*;

use super::{ActorInbox, MultiActorInbox};
use crate::{
    all::*,
    handler::{Event, HandlerState},
};
use event_listener::EventListener;
use futures::{future::BoxFuture, stream::FusedStream, FutureExt, Stream, StreamExt};
use std::{
    fmt::Debug,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
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

    fn channel_ref(&self) -> &Arc<<Self::ActorType as ActorType>::Channel> {
        &self.channel
    }
}

impl<P: Protocol + Send> MultiActorInbox for Inbox<P> {
    fn init_multi_inbox(
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

    fn from_channel(channel: Arc<Self::Channel>) -> Self {
        Self::new(channel)
    }
}

impl<P: Protocol + Send> ActorInbox for Inbox<P> {
    type Config = Capacity;

    fn init_single_inbox(
        config: Capacity,
        address_count: usize,
        actor_id: ActorId,
    ) -> (Arc<Self::Channel>, Self) {
        let channel = Self::init_multi_inbox(config, 1, address_count, actor_id);
        (channel.clone(), Self::new(channel))
    }
}

impl<P, H> HandlerState<H> for Inbox<P>
where
    P: Protocol + HandledBy<H>,
    H: Handler<State = Self>,
{
    type Protocol = P;
    type InboxType = Self;

    fn from_inbox(inbox: Self::InboxType) -> Self {
        inbox
    }

    fn poll_next_action(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<HandlerItem<H>> {
        self.poll_next_unpin(cx).map(|res| match res {
            Some(res) => match res {
                Ok(protocol) => HandlerItem::Protocol(protocol),
                Err(_halted) => HandlerItem::Event(Event::Halt),
            },
            None => HandlerItem::Event(Event::Dead),
        })
    }
}

fn unwrap_then_cancel<P: FromPayload<M>, M: Message>(prot: P, returned: M::Returned) -> M {
    let Ok(sent) = prot.try_into_payload() else {
        panic!("")
    };
    M::cancel(sent, returned)
}

impl<M, P> Accept<M> for Inbox<P>
where
    P: Protocol + FromPayload<M>,
    M: Message + Send + 'static,
    M::Payload: Send,
    M::Returned: Send,
{
    type SendFut<'a> = BoxFuture<'a, Result<M::Returned, SendError<M>>>;

    fn try_send(address: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>> {
        let (sends, returns) = M::create(msg);

        match address.try_send_protocol(P::from_payload(sends)) {
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

        match address.send_protocol_now(P::from_payload(sends)) {
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

        match address.send_protocol_blocking(P::from_payload(sends)) {
            Ok(()) => Ok(returns),
            Err(SendError(prot)) => Err(SendError(unwrap_then_cancel(prot, returns))),
        }
    }

    fn send(address: &Self::Channel, msg: M) -> Self::SendFut<'_> {
        Box::pin(async move {
            let (sends, returns) = M::create(msg);

            match address.send_protocol(P::from_payload(sends)).await {
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
        let this = &mut *self;

        let result = loop {
            let recv_listener = this
                .recv_listener
                .get_or_insert(this.channel.get_recv_listener());

            match this.channel.try_recv(&mut this.signaled_halt) {
                Ok(msg) => break Poll::Ready(Some(Ok(msg))),
                Err(error) => match error {
                    TryRecvError::Halted => break Poll::Ready(Some(Err(HaltedError))),
                    TryRecvError::ClosedAndEmpty => break Poll::Ready(None),
                    TryRecvError::Empty => {
                        ready!(recv_listener.poll_unpin(cx));
                        this.recv_listener = None;
                    }
                },
            };
        };

        if result.is_ready() {
            this.recv_listener = None;
        }
        result
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

use super::{ActorInbox, MultiActorInbox};
use crate::{
    all::*,
    handler::{Event, HandlerState},
};
use event_listener::EventListener;
use futures::{stream::FusedStream, Future, FutureExt, Stream, StreamExt};
use std::{
    fmt::Debug,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};

mod channel;
pub use channel::*;

/// The standard [`ActorInbox`] implemented as an mpmc-channel. Any messages that the [`Protocol`]
/// `P` accepts can be sent to this actor. This inbox also allows for multiple processes to be spawned
/// onto a single actor.
///
/// Messages may be received with [`Inbox::recv`], [`Inbox::try_recv`] or by using [`Stream`].
///
/// This also implements [`HandlerState`], which allows this to be used as a [`Handler::State`].
#[derive(Debug)]
pub struct Inbox<P: Protocol> {
    channel: Arc<InboxChannel<P>>,
    halted: bool,
    recv_listener: Option<EventListener>,
}

impl<P: Protocol> Inbox<P> {
    pub(crate) fn from_channel(channel: Arc<InboxChannel<P>>) -> Self {
        Inbox {
            channel,
            halted: false,
            recv_listener: None,
        }
    }

    /// Whether this inbox has been halted
    pub fn halted(&self) -> bool {
        self.halted
    }

    /// Attempt to receive a message from the channel.
    pub fn try_recv(&mut self) -> Result<P, TryRecvError> {
        self.channel.try_recv(&mut self.halted)
    }

    /// Receive a message from the channel, waiting for one to appear.
    pub fn recv(&mut self) -> RecvFut<'_, P> {
        self.channel.recv(&mut self.halted, &mut self.recv_listener)
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
        (channel.clone(), Self::from_channel(channel))
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
        Self::from_channel(channel)
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

impl<P: Protocol> ActorType for Inbox<P> {
    type Channel = InboxChannel<P>;
}

impl<P: Protocol> ActorRef for Inbox<P> {
    type ActorType = Self;

    fn channel_ref(this: &Self) -> &Arc<<Self::ActorType as ActorType>::Channel> {
        &this.channel
    }
}

impl<P: Protocol> Stream for Inbox<P> {
    type Item = Result<P, Halted>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = &mut *self;

        let result = loop {
            let recv_listener = this
                .recv_listener
                .get_or_insert(this.channel.get_recv_listener());

            match this.channel.try_recv(&mut this.halted) {
                Ok(msg) => break Poll::Ready(Some(Ok(msg))),
                Err(error) => match error {
                    TryRecvError::Halted => break Poll::Ready(Some(Err(Halted))),
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

impl<M, P> Accept<M> for Inbox<P>
where
    P: Protocol + FromPayload<M>,
    M: Message,
    M::Returned: Send,
{
    type SendFut<'a> = InboxSendFut<'a, P, M>;

    fn try_send(channel: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>> {
        let (sends, returns) = M::create(msg);

        match channel.try_send_protocol(P::from_payload(sends)) {
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

    fn force_send(channel: &Self::Channel, msg: M) -> Result<M::Returned, TrySendError<M>> {
        let (sends, returns) = M::create(msg);

        match channel.send_protocol_now(P::from_payload(sends)) {
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

    fn send_blocking(channel: &Self::Channel, msg: M) -> Result<M::Returned, SendError<M>> {
        let (sends, returns) = M::create(msg);

        match channel.send_protocol_blocking(P::from_payload(sends)) {
            Ok(()) => Ok(returns),
            Err(SendError(prot)) => Err(SendError(unwrap_then_cancel(prot, returns))),
        }
    }

    fn send(channel: &Self::Channel, msg: M) -> InboxSendFut<'_, P, M> {
        let (payload, returned) = M::create(msg);
        InboxSendFut {
            returned: Some(returned),
            prot_fut: channel.send_protocol(P::from_payload(payload)),
        }
    }
}

//------------------------------------------------------------------------------------------------
//  InboxSendFut
//------------------------------------------------------------------------------------------------

#[derive(Debug)]
pub struct InboxSendFut<'a, P: Protocol, M: Message> {
    returned: Option<M::Returned>,
    prot_fut: SendProtocolFut<'a, P>,
}

impl<'a, P, M> Future for InboxSendFut<'a, P, M>
where
    P: Protocol + FromPayload<M>,
    M: Message,
{
    type Output = Result<M::Returned, SendError<M>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.prot_fut.poll_unpin(cx).map(|res| match res {
            Ok(()) => Ok(self.returned.take().unwrap()),
            Err(SendError(protocol)) => Err(SendError(unwrap_then_cancel(
                protocol,
                self.returned.take().unwrap(),
            ))),
        })
    }
}

impl<'a, P: Protocol, M: Message> Unpin for InboxSendFut<'a, P, M> {}

fn unwrap_then_cancel<P: FromPayload<M>, M: Message>(prot: P, returned: M::Returned) -> M {
    let Ok(sent) = prot.try_into_payload() else {
        panic!("")
    };
    M::cancel(sent, returned)
}

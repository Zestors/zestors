use crate::halter_channel::HalterChannel;
use event_listener::EventListener;
use futures::{ready, Future, FutureExt};
use std::{sync::Arc, task::Poll};
use zestors_core::*;

/// A halter can be used for processes that do not handle any messages, but that should still be
/// supervisable. The halter can be awaited, and returns when the task should halt.
pub struct Halter {
    channel: Arc<HalterChannel>,
    halt_event_listener: Option<EventListener>,
    halted: bool,
}

impl Halter {
    /// Whether this task has been halted.
    pub fn halted(&self) -> bool {
        self.halted
    }
}

impl ActorRef for Halter {
    type ChannelDefinition = Self;

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

impl Spawn for Halter {
    type Config = Link;

    fn setup_channel(
        link: Self::Config,
        halter_count: usize,
        address_count: usize,
        actor_id: ActorId,
    ) -> (
        Arc<<Self::ChannelDefinition as DefineChannel>::Channel>,
        Link,
    ) {
        (
            Arc::new(HalterChannel::new(address_count, halter_count, actor_id)),
            link,
        )
    }

    fn new(channel: Arc<<Self::ChannelDefinition as DefineChannel>::Channel>) -> Self {
        Self {
            channel,
            halt_event_listener: None,
            halted: false,
        }
    }
}

impl Unpin for Halter {}

impl Future for Halter {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // If we have already halted before, then return immeadeately..
        if self.halted {
            return Poll::Ready(());
        }
        loop {
            // Acquire a halt listener if not set.
            let listener = if let Some(listener) = &mut self.halt_event_listener {
                listener
            } else {
                self.halt_event_listener = Some(self.channel.get_halt_listener());
                self.halt_event_listener.as_mut().unwrap()
            };

            // If it is pending return, otherwise remove the listener.
            ready!(listener.poll_unpin(cx));
            self.halt_event_listener = None;

            if self.channel.should_halt() {
                break Poll::Ready(());
            }
        }
    }
}

impl Drop for Halter {
    fn drop(&mut self) {
        self.channel.decrement_halter_count(1)
    }
}

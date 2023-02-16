mod channel;
pub use channel::*;

use crate::all::*;
use event_listener::EventListener;
use futures::{ready, Future, FutureExt};
use std::{sync::Arc, task::Poll};

/// A halter can be used for processes that do not handle any messages, but that should still be
/// supervisable. The halter can be awaited, and returns when the task should halt.
///
/// For a halter that can contain multiple processes, see [PoolHalter]
pub struct Halter {
    channel: Arc<HalterChannel>,
    halt_event_listener: Option<EventListener>,
}

impl Halter {
    pub fn to_halt(&self) -> bool {
        self.channel.to_halt()
    }
}

impl ActorType for Halter {
    type Channel = HalterChannel;
}

impl InboxType for Halter {
    type Config = ();

    fn setup_channel(
        _config: (),
        address_count: usize,
        actor_id: ActorId,
    ) -> Arc<Self::Channel> {
        Arc::new(HalterChannel::new(address_count, actor_id))
    }

    fn from_channel(channel: Arc<Self::Channel>) -> Self {
        Self {
            channel,
            halt_event_listener: None,
        }
    }
}

impl ChannelRef for Halter {
    type ActorType = Self;
    fn channel(&self) -> &Arc<<Self as ActorType>::Channel> {
        &self.channel
    }
}

impl Unpin for Halter {}

impl Future for Halter {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        loop {
            // If we have already halted before, then return immeadeately..
            if self.to_halt() {
                return Poll::Ready(());
            }

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
        }
    }
}

impl Drop for Halter {
    fn drop(&mut self) {
        self.channel.exit();
    }
}

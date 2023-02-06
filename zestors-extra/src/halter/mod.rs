mod halter_channel;
pub use halter_channel::*;

use event_listener::EventListener;
use futures::{ready, Future, FutureExt};
use std::{sync::Arc, task::Poll};
use zestors_core::{
    actor_type::ActorType, inboxes::InboxType, monitoring::ActorId, spawning::ActorRef,
};

/// A halter can be used for processes that do not handle any messages, but that should still be
/// supervisable. The halter can be awaited, and returns when the task should halt.
/// 
/// For a halter that can contain multiple processes, see [GroupHalter]
pub struct Halter {
    channel: Arc<HalterChannel>,
    halt_event_listener: Option<EventListener>,
}

impl Halter {
    pub fn should_halt(&self) -> bool {
        self.channel.should_halt()
    }
}

impl ActorType for Halter {
    type Channel = HalterChannel;
}

impl ActorRef for Halter {
    type ActorType = Self;
    fn channel(&self) -> &Arc<<Self::ActorType as ActorType>::Channel> {
        &self.channel
    }
}

impl InboxType for Halter {
    type Config = ();

    fn setup_channel(
        _config: (),
        address_count: usize,
        actor_id: ActorId,
    ) -> Arc<<Self::ActorType as ActorType>::Channel> {
        Arc::new(HalterChannel::new(address_count, actor_id))
    }

    fn from_channel(channel: Arc<<Self::ActorType as ActorType>::Channel>) -> Self {
        Self {
            channel,
            halt_event_listener: None,
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
        loop {
            // If we have already halted before, then return immeadeately..
            if self.should_halt() {
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

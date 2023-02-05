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
pub struct Halter {
    channel: Arc<CoreChannel>,
    halt_event_listener: Option<EventListener>,
    halted: bool,
}

impl Halter {
    /// Whether this task has been halted.
    pub fn halted(&self) -> bool {
        self.halted
    }
}

impl ActorType for Halter {
    type Channel = CoreChannel;
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
        link: Self::Config,
        halter_count: usize,
        address_count: usize,
        actor_id: ActorId,
    ) -> Arc<<Self::ActorType as ActorType>::Channel> {
        Arc::new(CoreChannel::new(address_count, halter_count, actor_id))
    }

    fn new(channel: Arc<<Self::ActorType as ActorType>::Channel>) -> Self {
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
        self.channel.remove_process();
    }
}

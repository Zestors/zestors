mod channel;
pub use channel::*;

use event_listener::EventListener;
use futures::{ready, Future, FutureExt};
use std::{sync::Arc, task::Poll};
use crate::all::*;

/// A halter can be used for processes that do not handle any messages, but that should still be
/// supervisable. The halter can be awaited and resolves when the task should halt.
///
/// For a halter that can only spawn a single processes, see [`Halter`].
pub struct MultiHalter {
    channel: Arc<MultiHalterChannel>,
    halt_event_listener: Option<EventListener>,
    halted: bool,
}

impl MultiHalter {
    /// Whether this task has been halted.
    pub fn halted(&self) -> bool {
        self.halted
    }
}

impl ActorType for MultiHalter {
    type Channel = MultiHalterChannel;
}

impl ActorRef for MultiHalter {
    type ActorType = Self;
    fn channel_ref(this: &Self) -> &Arc<<Self::ActorType as ActorType>::Channel> {
        &this.channel
    }
}

impl MultiActorInbox for MultiHalter {
    fn init_multi_inbox(
        _config: Self::Config,
        process_count: usize,
        address_count: usize,
        actor_id: ActorId,
    ) -> Arc<Self::Channel> {
        Arc::new(MultiHalterChannel::new(address_count, process_count, actor_id))
    }

    fn from_channel(channel: Arc<Self::Channel>) -> Self {
        Self {
            channel,
            halt_event_listener: None,
            halted: false,
        }
    }
}

impl ActorInbox for MultiHalter {
    type Config = ();

    fn init_single_inbox(
        config: Self::Config,
        address_count: usize,
        actor_id: ActorId,
    ) -> (Arc<Self::Channel>, Self) {
        let channel = Self::init_multi_inbox(config, 1, address_count, actor_id);
        (channel.clone(), Self::from_channel(channel))
    }


}

impl Unpin for MultiHalter {}

impl Future for MultiHalter {
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

impl Drop for MultiHalter {
    fn drop(&mut self) {
        self.channel.decrement_halter_count(1)
    }
}

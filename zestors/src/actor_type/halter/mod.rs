use crate::all::*;
use event_listener::EventListener;
use futures::{ready, Future, FutureExt};
use std::{sync::Arc, task::Poll};

mod channel;
pub use channel::*;

/// A halter can be used for processes that do not handle any messages, but that should still be
/// supervisable. The halter can be awaited and resolves when the task should halt.
///
/// For a halter that can spawn multiple processes, see [`MultiHalter`].
pub struct Halter {
    channel: Arc<HalterChannel>,
    halt_event_listener: Option<EventListener>,
}

impl Halter {
    /// Whether this process should halt.
    pub fn halted(&self) -> bool {
        self.channel.halted()
    }
}

impl ActorType for Halter {
    type Channel = HalterChannel;
}

impl InboxType for Halter {
    type Config = ();

    fn init_single_inbox(
        _config: (),
        address_count: usize,
        actor_id: ActorId,
    ) -> (Arc<Self::Channel>, Self) {
        let channel = Arc::new(HalterChannel::new(address_count, actor_id));
        (
            channel.clone(),
            Self {
                channel,
                halt_event_listener: None,
            },
        )
    }
}

impl ActorRef for Halter {
    type ActorType = Self;
    fn channel_ref(this: &Self) -> &Arc<<Self as ActorType>::Channel> {
        &this.channel
    }
}

impl Future for Halter {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        loop {
            // If we have already halted before, then return immeadeately..
            if self.halted() {
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

impl Unpin for Halter {}

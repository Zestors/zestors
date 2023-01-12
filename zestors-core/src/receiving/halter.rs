use crate::*;
use event_listener::EventListener;
use futures::{ready, Future, FutureExt};
use std::{sync::Arc, task::Poll};

/// A halter can be used for processes that do not handle any messages, but that should still be
/// supervisable. The halter can be awaited, and returns when the task should halt.
pub struct Halter {
    channel: Arc<HalterChannel>,
    halt_event_listener: Option<EventListener>,
    halted: bool,
}

impl Halter {
    /// This does NOT increase the halter-count.
    pub(crate) fn from_channel(channel: Arc<HalterChannel>) -> Self {
        Self {
            channel,
            halt_event_listener: None,
            halted: false,
        }
    }

    /// Whether this task has been halted.
    pub fn halted(&self) -> bool {
        self.halted
    }
}

impl SpawnsWith for Halter {
    type ChannelDefinition = Self;
    type Config = Link;

    fn setup_channel(
        link: Self::Config,
        halter_count: usize,
        address_count: usize,
    ) -> (Arc<<Self::ChannelDefinition as DefinesChannel>::Channel>, Link) {
        (
            Arc::new(HalterChannel::new(address_count, halter_count)),
            link,
        )
    }

    fn new(channel: Arc<<Self::ChannelDefinition as DefinesChannel>::Channel>) -> Self {
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

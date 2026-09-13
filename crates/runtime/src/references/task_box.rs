use crate::*;
use std::convert::Infallible;

/// A [`TaskBox`] is a wrapper around an [`Inbox<Infallible>`] that can only receive signals.
#[derive(Debug)]
pub struct TaskBox {
    inbox: Inbox<Infallible>,
}

impl TaskBox {
    pub fn new(inbox: Inbox<Infallible>) -> Self {
        Self { inbox }
    }

    /// Returns the next signal from the channel, or `None` if the channel has received
    /// On the first call to `next`, the channel's status will be set to
    /// [`Running`](ActorStatus::Running), and will count as a completion of the initialization
    /// phase.
    pub async fn next(&mut self) -> Option<Signal> {
        match self.inbox.next().await? {
            InboxEvent::Signal(signal) => Some(signal),
            InboxEvent::Message(msg) => match msg {},
        }
    }

    pub fn try_next(&mut self) -> Option<Signal> {
        match self.inbox.try_next()? {
            InboxEvent::Signal(signal) => Some(signal),
            InboxEvent::Message(msg) => match msg {},
        }
    }

    /// Waits for a [`Signal::Shutdown`] signal to be received, and then returns.
    pub async fn wait_shutdown(&mut self) {
        if self.is_shutting_down() {
            return;
        }

        while let Some(signal) = self.next().await {
            if signal == Signal::Shutdown {
                break;
            }
        }
    }

    pub fn is_shutting_down(&self) -> bool {
        self.status() == ActorStatus::Stopping
    }

    pub async fn run_until_shutdown<O>(
        &mut self,
        fut: impl Future<Output = O> + Send,
    ) -> Result<O, Cancelled> {
        self.inbox.run_until_shutdown(fut).await
    }
}

impl ActorRef for TaskBox {
    type Ctx = Infallible;

    fn channel(&self) -> &Channel<Self::Ctx> {
        &self.inbox.channel()
    }
}

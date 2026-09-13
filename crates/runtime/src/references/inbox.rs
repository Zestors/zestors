use crate::*;
use std::convert::Infallible;

/// A reference to a [`Channel`] that can be used to receive messages and signals from the channel.
///
/// This is a strong reference ([`StrongAddress`]) to the channel, which means that it will keep
/// the channel alive as long as it exists.
///
/// See the [`Inbox::next`] method for receiving messages and signals from the channel.
#[derive(Debug)]
pub struct Inbox<T: Interface> {
    channel: StrongAddress<T>,
    init: InitState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitState {
    Auto,
    Manual,
    Completed,
}

impl Inbox<Infallible> {
    pub fn into_task_box(self) -> TaskBox {
        TaskBox::new(self)
    }
}

impl<T: Interface> Inbox<T> {
    pub(crate) fn try_new(channel: StrongAddress<T>) -> Result<Self, ConcurrentInboxError> {
        if !channel.status().is_dead() {
            return Err(ConcurrentInboxError);
        }

        let inbox = Self {
            channel,
            init: InitState::Auto,
        };

        Ok(inbox)
    }

    /// Returns the next event from the channel, or `None` if the channel has received
    /// a [`Signal::Shutdown`] signal and has no more messages to process.
    ///
    /// If the channel is [`Suspended`](ActorStatus::Suspended), this method will only
    /// receive [signals](Signal). Only after receiving a [`Signal::Resume`] signal will
    /// it start receiving messages again.
    ///
    /// Upon the first call to `next`, the channel's status will be set to
    /// [`Running`](ActorStatus::Running), and will count as a completion of the initialization
    /// phase. For receiving signals without setting the status to running, see [`Inbox::next_signal`].
    pub async fn next(&mut self) -> Option<InboxEvent<T>> {
        // If this is the first call to next(), set the status to Running
        self.register_initialized();

        self.channel().next().await
    }

    fn init_completed(&self) -> bool {
        self.init == InitState::Completed
    }

    pub fn set_manual_init(&mut self) -> bool {
        if self.init_completed() {
            return false;
        }

        self.init = InitState::Manual;
        true
    }

    pub fn set_auto_init(&mut self) -> bool {
        if self.init_completed() {
            return false;
        }

        self.init = InitState::Auto;
        true
    }

    pub fn register_initialized(&mut self) -> bool {
        if self.init_completed() {
            return false;
        }

        self.init = InitState::Completed;
        self.channel().register_initialized().unwrap_or(false)
    }

    pub fn register_stopping(&mut self) -> bool {
        self.channel().register_stopping().unwrap_or(false)
    }

    // TODO: Remove this method in a future version
    pub async fn next_with_init(&mut self, init: bool) -> Option<InboxEvent<T>> {
        match init {
            true => self.next().await,
            false => self.channel().next().await,
        }
    }

    pub fn try_next(&mut self) -> Option<InboxEvent<T>> {
        self.register_initialized();
        self.channel().try_next()
    }

    /// Returns the next signal from the channel, or `None` if the channel has received.
    pub async fn next_signal(&mut self) -> Option<Signal> {
        self.channel().recv_signal().await
    }

    pub fn is_shutting_down(&self) -> bool {
        self.status() == ActorStatus::Stopping
    }

    async fn wait_resume(&mut self) {
        while let Some(signal) = self.next_signal().await {
            match signal {
                Signal::Resume | Signal::Shutdown => break,
                _ => {}
            }
        }
    }

    pub async fn run_until_shutdown<O>(
        &mut self,
        fut: impl Future<Output = O> + Send,
    ) -> Result<O, Cancelled> {
        if self.is_shutting_down() {
            return Err(Cancelled);
        }

        tokio::pin!(fut);

        loop {
            // If we are currently suspended, pause before polling `fut` again
            if self.status() == ActorStatus::Suspended {
                self.wait_resume().await;
                if self.is_shutting_down() {
                    return Err(Cancelled);
                }
            }

            tokio::select! {
                res = &mut fut => return Ok(res),
                signal = self.next_signal() => match signal {
                    Some(Signal::Shutdown) | None => return Err(Cancelled),
                    Some(Signal::Suspend) => {
                        self.wait_resume().await;
                        if self.is_shutting_down() {
                            return Err(Cancelled);
                        }
                    }
                    Some(_) => {}
                }
            }
        }
    }
}

impl<T: Interface> ActorRef for Inbox<T> {
    type Ctx = T;

    fn channel(&self) -> &Channel<Self::Ctx> {
        &self.channel.channel()
    }
}

impl<T: Interface> Drop for Inbox<T> {
    fn drop(&mut self) {
        self.channel().drain_messages_and_signals();
    }
}

pub enum InboxEvent<M> {
    Signal(Signal),
    Message(M),
}

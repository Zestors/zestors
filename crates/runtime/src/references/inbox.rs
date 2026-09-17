use crate::*;
use std::convert::Infallible;

/// A reference to an actor's channel that can be used to receive messages and signals from it.
///
/// This is a strong reference ([`StrongAddress`]) to the channel, which means that it will keep
/// the channel alive as long as it exists.
///
/// See the [`Inbox::recv_event`] method for receiving messages and signals from the channel.
#[derive(Debug)]
pub struct Inbox<T: Interface> {
    address: StrongAddress<T>,
    init: InitState,
}

/// Controls when an [`Inbox`] transitions its channel from
/// [`ActorStatus::Initializing`] to [`ActorStatus::Running`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitState {
    /// The default: the transition happens automatically, the first time any
    /// `recv*`/`try_recv` method is called.
    Auto,
    /// Set via [`Inbox::set_manual_init`]: the automatic transition is
    /// suppressed, and the caller is responsible for calling
    /// [`Inbox::register_initialized`] once the actor considers itself ready
    /// (e.g. after a supervisor has finished starting its initial children).
    Manual,
    /// The transition has already happened; further calls are no-ops.
    Completed,
}

impl Inbox<Infallible> {
    pub fn into_task_box(self) -> TaskBox {
        TaskBox::new(self)
    }
}

impl<T: Interface> Inbox<T> {
    pub(crate) fn try_new(address: StrongAddress<T>) -> Result<Self, ConcurrentInboxError> {
        if !address.status().is_dead() {
            return Err(ConcurrentInboxError);
        }

        let inbox = Self {
            address,
            init: InitState::Auto,
        };

        Ok(inbox)
    }

    /// Returns the next event from the channel, or `None` if the channel has received
    /// a [`Signal::Shutdown`] signal and has no more messages to process.
    pub async fn recv_event(&mut self) -> Option<InboxEvent<T>> {
        self.maybe_auto_init();
        self.as_address().next_event(false).await
    }

    /// Same as [`Inbox::recv_event`], but keeps returning signals (as
    /// [`InboxEvent::Signal`]) after a [`Signal::Shutdown`] has been received,
    /// instead of returning `None` once the inbox is empty.
    pub async fn recv_event_always(&mut self) -> Option<InboxEvent<T>> {
        self.maybe_auto_init();
        self.as_address().next_event(true).await
    }

    /// Block until the next message becomes available. Once the actor has received a
    /// [`Signal::Shutdown`], the inbox will be closed, and all remaining messages
    /// are received until the inbox is empty.
    pub async fn recv(&mut self) -> Option<T> {
        self.maybe_auto_init();

        loop {
            match self.as_address().next_event(false).await {
                Some(ev) => {
                    if let InboxEvent::Message(msg) = ev {
                        return Some(msg);
                    }
                }
                None => return None,
            }
        }
    }

    /// Non-blocking version of [`Inbox::recv_event`]: returns `None` immediately
    /// if no message or signal is currently available.
    pub fn try_recv(&mut self) -> Option<InboxEvent<T>> {
        self.maybe_auto_init();
        self.as_address().try_next_event()
    }

    /// Block until the next signal becomes available, ignoring any queued messages.
    pub async fn recv_signal(&mut self) -> Option<Signal> {
        self.maybe_auto_init();
        self.as_address().next_signal().await
    }

    fn init_completed(&self) -> bool {
        self.init == InitState::Completed
    }

    /// Completes initialization automatically, but only if it is still in the
    /// default [`InitState::Auto`] mode. Called by every `recv*`/`try_recv`
    /// method; a no-op once [`Inbox::set_manual_init`] has been called, or
    /// once initialization has already completed.
    fn maybe_auto_init(&mut self) {
        if self.init == InitState::Auto {
            self.register_initialized();
        }
    }

    /// Switches to manual initialization: the channel will no longer
    /// transition from [`ActorStatus::Initializing`] to
    /// [`ActorStatus::Running`] automatically on the first `recv*`/`try_recv`
    /// call. The caller becomes responsible for calling
    /// [`Inbox::register_initialized`] once the actor is ready to be
    /// considered running (for example, once a supervisor has finished
    /// starting its initial children).
    ///
    /// Returns `false` without effect if initialization has already completed.
    pub fn set_manual_init(&mut self) -> bool {
        if self.init_completed() {
            return false;
        }

        self.init = InitState::Manual;
        true
    }

    /// Switches back to automatic initialization (the default): the next
    /// `recv*`/`try_recv` call will complete initialization. See
    /// [`Inbox::set_manual_init`].
    ///
    /// Returns `false` without effect if initialization has already completed.
    pub fn set_auto_init(&mut self) -> bool {
        if self.init_completed() {
            return false;
        }

        self.init = InitState::Auto;
        true
    }

    /// Explicitly completes initialization, transitioning the channel from
    /// [`ActorStatus::Initializing`] to [`ActorStatus::Running`].
    ///
    /// Under the default auto-init mode (see [`Inbox::set_auto_init`]) this is
    /// called automatically by every `recv*`/`try_recv` method and rarely
    /// needs to be called directly. After [`Inbox::set_manual_init`], this is the only thing that
    /// completes initialization.
    ///
    /// Returns `false` without effect if initialization has already completed.
    pub fn register_initialized(&mut self) -> bool {
        if self.init_completed() {
            return false;
        }

        self.init = InitState::Completed;
        self.as_address().register_initialized().unwrap_or(false)
    }

    /// Transitions the channel to [`ActorStatus::Exiting`], as if a
    /// [`Signal::Shutdown`] had been received. Returns `false` if the channel
    /// was already exiting or dead.
    pub fn register_exiting(&mut self) -> bool {
        self.as_address().register_exiting().unwrap_or(false)
    }

    async fn wait_resume(&mut self) {
        while let Some(signal) = self.recv_signal().await {
            match signal {
                Signal::Resume | Signal::Shutdown => break,
                _ => {}
            }
        }
    }

    /// Runs `fut` to completion while responding to signals: a
    /// [`Signal::Suspend`] pauses polling `fut` until a [`Signal::Resume`] or
    /// [`Signal::Shutdown`] is received, and a [`Signal::Shutdown`] cancels
    /// `fut` and returns [`Cancelled`].
    pub async fn run_until_shutdown<O>(
        &mut self,
        fut: impl Future<Output = O> + Send,
    ) -> Result<O, Cancelled> {
        if self.is_exiting() {
            return Err(Cancelled);
        }

        tokio::pin!(fut);

        loop {
            // If we are currently suspended, pause before polling `fut` again
            if self.status() == ActorStatus::Suspended {
                self.wait_resume().await;
                if self.is_exiting() {
                    return Err(Cancelled);
                }
            }

            tokio::select! {
                res = &mut fut => return Ok(res),
                signal = self.recv_signal() => match signal {
                    Some(Signal::Shutdown) | None => return Err(Cancelled),
                    Some(Signal::Suspend) => {
                        self.wait_resume().await;
                        if self.is_exiting() {
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

    fn as_address(&self) -> &Address<Self::Ctx> {
        self.address.as_address()
    }
}

impl<T: Interface> Drop for Inbox<T> {
    fn drop(&mut self) {
        self.as_address().drain_messages_and_signals();
    }
}

pub enum InboxEvent<M> {
    Signal(Signal),
    Message(M),
}

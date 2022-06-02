use crate::core::*;
use futures::{FutureExt, Stream, StreamExt};
use std::{sync::Arc, task::Poll};

//------------------------------------------------------------------------------------------------
//  State
//------------------------------------------------------------------------------------------------

/// The state of an actor contains all information necessary to run the event-loop of the actor.
/// This is primarily:
///
/// * The inbox, used to receive messages from other processes.
/// * The signal receiver, used to receive the abort message.
/// * State of the process, for example it's process_id.
/// * State of the event-loop, for example it's priorities when handling events.
///
/// The state can be used to manually receive/handle messages, for example when the actor is exiting.
/// It can also be used to change actor-behaviour at runtime.
///
/// In general, it is not necessary to use the `State`, since the actor's event-loop takes care of
/// this for you.
#[derive(Debug)]
pub struct State<A> {
    receiver: async_channel::Receiver<InternalMsg<A>>,
    rcv_signal: Option<Rcv<StateSignal>>,
    actor_scheduler_enabled: bool,
    shared: Arc<SharedProcessData>,
}

impl<A> State<A> {
    pub(crate) fn new(
        receiver: async_channel::Receiver<InternalMsg<A>>,
        rcv_signal: Rcv<StateSignal>,
        shared: Arc<SharedProcessData>,
    ) -> Self {
        Self {
            actor_scheduler_enabled: true,
            receiver,
            rcv_signal: Some(rcv_signal),
            shared,
        }
    }

    /// Whether the scheduler is enabled.
    pub fn scheduler_enabled(&self) -> bool {
        self.actor_scheduler_enabled
    }

    /// Enables the scheduler to run.
    pub fn enable_scheduler(&mut self) {
        self.actor_scheduler_enabled = true
    }

    /// Disable the scheduler from running.
    pub fn disable_scheduler(&mut self) {
        self.actor_scheduler_enabled = false
    }

    /// Checks whether the inbox is closed.
    pub fn is_closed(&self) -> bool {
        self.receiver.is_closed()
    }

    /// Amount of (local) addresses to this actor.
    pub fn addr_count(&self) -> usize {
        self.receiver.sender_count()
    }

    /// Amount of messages in the inbox.
    pub fn msg_count(&self) -> usize {
        self.receiver.len()
    }

    /// Close the inbox for this actor.
    ///
    /// Any messages already sent can still be received, however no new messages may be
    /// sent.
    ///
    /// Signal can still be received after the inbox is emptied with `recv_signal`.
    pub fn close(&mut self) -> bool {
        self.receiver.close()
    }

    /// Get the unique process id.
    pub fn process_id(&self) -> ProcessId {
        self.shared.process_id()
    }

    /// Receive only an incoming signal, but leave messages in the inbox.
    ///
    /// Returns an error if the signal has already been recieved
    pub async fn recv_signal(&mut self) -> Result<StateSignal, InboxSignalRecvError> {
        if let Some(rcv_signal) = self.rcv_signal.as_mut() {
            Ok(rcv_signal.await.unwrap())
        } else {
            Err(InboxSignalRecvError)
        }
    }

    /// Same as recv_signal, but returns `None` if no message is in the inbox.
    pub fn try_recv_signal(&mut self) -> Result<Option<StateSignal>, InboxSignalRecvError> {
        if let Some(rcv_signal) = self.rcv_signal.as_mut() {
            Ok(rcv_signal.try_recv().unwrap())
        } else {
            Err(InboxSignalRecvError)
        }
    }

    /// Receive the next message for this actor. This can be either:
    /// * `StateSignal`: Signaling that this actor is isolated or should abort.
    /// * `Action`: Message sent to this actor that should be handled.
    ///
    /// Receiving here does not trigger the `Scheduler`, since that is located withing
    /// the actor itself.
    ///
    /// `Signal`s always have priority over `Action`s when receiving.
    pub async fn recv(&mut self) -> Result<StateMsg<A>, InboxRecvError> {
        self.next().await.ok_or(InboxRecvError)
    }

    /// Same as `rcv`, but returns `None` if no message is in the inbox.
    pub fn try_recv(&mut self) -> Result<Option<StateMsg<A>>, InboxRecvError> {
        if let Some(rcv_signal) = self.rcv_signal.as_mut() {
            match rcv_signal.try_recv() {
                Ok(Some(signal)) => {
                    self.rcv_signal.take().unwrap();
                    return Ok(Some(StateMsg::Signal(signal)));
                }
                Ok(None) => (),
                Err(_) => unreachable!("Request never be dropped/canceled"),
            }
        }

        match self.receiver.try_recv() {
            Ok(InternalMsg::Action(action)) => Ok(Some(StateMsg::Action(action))),
            Err(e) => match e {
                async_channel::TryRecvError::Empty => Ok(None),
                async_channel::TryRecvError::Closed => Err(InboxRecvError),
            },
        }
    }
}

impl<A> Stream for State<A> {
    type Item = StateMsg<A>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        if let Some(rcv_signal) = &mut self.rcv_signal {
            match rcv_signal.poll_unpin(cx) {
                Poll::Ready(signal) => {
                    self.rcv_signal.take().unwrap();
                    Poll::Ready(Some(StateMsg::Signal(signal.unwrap())))
                }

                Poll::Pending => self.receiver.poll_next_unpin(cx).map(|val| {
                    val.map(|action| {
                        let InternalMsg::Action(action) = action;
                        StateMsg::Action(action)
                    })
                }),
            }
        } else {
            self.receiver.poll_next_unpin(cx).map(|val| {
                val.map(|action| {
                    let InternalMsg::Action(action) = action;
                    StateMsg::Action(action)
                })
            })
        }
    }
}

impl<A> Drop for State<A> {
    fn drop(&mut self) {
        // Close the inbox.
        self.receiver.close();

        // Set exited to true.
        self.shared.exit();

        // And empty the inbox
        while self.try_recv().is_ok() {}
    }
}

//------------------------------------------------------------------------------------------------
//  Messages
//------------------------------------------------------------------------------------------------

/// Returned when receiving a message from a `State`.
///
/// Contains either an action or a signal.
#[derive(Debug)]
pub enum StateMsg<A> {
    Action(Action<A>),
    Signal(StateSignal),
}

/// Returned when receiving a signal from a `State`.
///
/// Contains either a soft-abort or an isolated signal.
#[derive(Debug, Clone)]
pub enum StateSignal {
    SoftAbort,
    Isolated,
}

pub(crate) enum InternalMsg<A> {
    Action(Action<A>),
}

//------------------------------------------------------------------------------------------------
//  Recv Errors
//------------------------------------------------------------------------------------------------

/// There are no more `Addr`esses coupled to this `Inbox`.
pub struct InboxRecvError;

/// Signal has already been received.
pub struct InboxSignalRecvError;

impl From<async_channel::RecvError> for InboxRecvError {
    fn from(_: async_channel::RecvError) -> Self {
        Self
    }
}

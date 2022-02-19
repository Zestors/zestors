use futures::{Stream, StreamExt};
use std::{pin::Pin, marker::PhantomData};

use crate::{actor::Actor, action::Action};

//--------------------------------------------------------------------------------------------------
//  Capacity
//--------------------------------------------------------------------------------------------------

/// This trait should not be implemented!
pub trait Capacity<B: InboxType>: InboxType {
    fn capacity(cap: usize) -> Option<usize>;
}

impl Capacity<Bounded> for Bounded {
    fn capacity(cap: usize) -> Option<usize> {
        Some(cap)
    }
}

impl Capacity<Unbounded> for Unbounded {
    fn capacity(_cap: usize) -> Option<usize> {
        None
    }
}

//--------------------------------------------------------------------------------------------------
//  Channel
//--------------------------------------------------------------------------------------------------

pub(crate) fn channel<A: Actor>(
    size: Option<usize>,
) -> (ActionSender<A>, ActionReceiver<A>) {
    let (sender, receiver) = match size {
        Some(size) => async_channel::bounded(size),
        None => async_channel::unbounded(),
    };

    let tx = ActionSender { sender };
    let rx = ActionReceiver { receiver };

    (tx, rx)
}

//--------------------------------------------------------------------------------------------------
//  InboxType
//--------------------------------------------------------------------------------------------------

/// This trait should not be implemented!
pub unsafe trait InboxType {}

unsafe impl InboxType for Unbounded {}
unsafe impl InboxType for Bounded {}

pub trait IsBounded {}
pub trait IsUnbounded {}

impl IsUnbounded for Unbounded {}
impl IsBounded for Bounded {}

/// Indicates (type-checked) that an [Actor] mailbox is unbounded.
pub struct Unbounded;

/// Indicates (type-checked) that an [Actor] mailbox is bounded by [Actor::INBOX_CAPACITY].
pub struct Bounded;

//--------------------------------------------------------------------------------------------------
//  Sender
//--------------------------------------------------------------------------------------------------

/// A sender of [Packet]s
#[derive(Debug)]
pub(crate) struct ActionSender<A: ?Sized + Actor> {
    sender: async_channel::Sender<Action<A>>,
}

impl<A: Actor> ActionSender<A> {
    pub(crate) fn is_alive(&self) -> bool {
        !self.sender.is_closed()
    }

    pub(crate) fn try_send(&self, packet: Action<A>) -> Result<(), ActionTrySendError<A>> {
        match self.sender.try_send(packet) {
            Ok(()) => Ok(()),
            Err(e) => match e {
                async_channel::TrySendError::Full(packet) => Err(ActionTrySendError::Full(packet)),
                async_channel::TrySendError::Closed(packet) => {
                    Err(ActionTrySendError::Disconnected(packet))
                }
            },
        }
    }

    pub(crate) async fn send_async(
        &self,
        action: Action<A>,
    ) -> Result<(), ActionSendError<A>> {
        match self.sender.send(action).await {
            Ok(()) => Ok(()),
            Err(e) => Err(ActionSendError::Disconnected(e.0)),
        }
    }
}

impl<A: Actor> Clone for ActionSender<A> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}



//--------------------------------------------------------------------------------------------------
//  Receiver
//--------------------------------------------------------------------------------------------------

/// A receiver of [Packet]s
#[derive(Debug)]
pub(crate) struct ActionReceiver<A: Actor> {
    pub receiver: async_channel::Receiver<Action<A>>,
}

impl<A: Actor> Stream for ActionReceiver<A> {
    type Item = Action<A>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.as_mut().receiver.poll_next_unpin(cx)
    }
}

impl<A: Actor> Drop for ActionReceiver<A> {
    fn drop(&mut self) {
        self.receiver.close();
        while let Ok(packet) = self.receiver.try_recv() {
            drop(packet)
        }
    }
}

//--------------------------------------------------------------------------------------------------
//  PacketSend errors
//--------------------------------------------------------------------------------------------------

pub(crate) enum ActionTrySendError<A: Actor> {
    Full(Action<A>),
    Disconnected(Action<A>),
}

pub(crate) enum ActionSendError<A: Actor> {
    Disconnected(Action<A>),
}

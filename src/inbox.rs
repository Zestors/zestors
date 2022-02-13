use futures::{Stream, StreamExt};
use std::pin::Pin;

use crate::{packet::Packet, actor::Actor};

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
//  PacketSender
//--------------------------------------------------------------------------------------------------

/// A sender of [Packet]s
#[derive(Debug)]
pub(crate) struct PacketSender<A: Actor + ?Sized> {
    sender: async_channel::Sender<Packet<A>>,
}

impl<A: Actor> PacketSender<A> {
    pub(crate) fn is_alive(&self) -> bool {
        !self.sender.is_closed()
    }

    pub(crate) fn try_send_packet(&self, packet: Packet<A>) -> Result<(), PacketTrySendError<A>> {
        match self.sender.try_send(packet) {
            Ok(()) => Ok(()),
            Err(e) => match e {
                async_channel::TrySendError::Full(packet) => Err(PacketTrySendError::Full(packet)),
                async_channel::TrySendError::Closed(packet) => {
                    Err(PacketTrySendError::Disconnected(packet))
                }
            },
        }
    }

    pub(crate) async fn send_packet_async(
        &self,
        packet: Packet<A>,
    ) -> Result<(), PacketSendError<A>> {
        match self.sender.send(packet).await {
            Ok(()) => Ok(()),
            Err(e) => Err(PacketSendError::Disconnected(e.0)),
        }
    }
}

impl<A: Actor> Clone for PacketSender<A> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

pub(crate) fn packet_channel<A: Actor>(
    size: Option<usize>,
) -> (PacketSender<A>, PacketReceiver<A>) {
    let (tx, rx) = match size {
        Some(size) => async_channel::bounded(size),
        None => async_channel::unbounded(),
    };

    let tx = PacketSender { sender: tx };
    let rx = PacketReceiver { receiver: rx };

    (tx, rx)
}

//--------------------------------------------------------------------------------------------------
//  PacketReceiver
//--------------------------------------------------------------------------------------------------

/// A receiver of [Packet]s
#[derive(Debug)]
pub(crate) struct PacketReceiver<A: Actor> {
    pub receiver: async_channel::Receiver<Packet<A>>,
}

impl<A: Actor> Stream for PacketReceiver<A> {
    type Item = Packet<A>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.as_mut().receiver.poll_next_unpin(cx)
    }
}

impl<A: Actor> Drop for PacketReceiver<A> {
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

pub(crate) enum PacketTrySendError<A: Actor> {
    Full(Packet<A>),
    Disconnected(Packet<A>),
}

pub(crate) enum PacketSendError<A: Actor> {
    Disconnected(Packet<A>),
}

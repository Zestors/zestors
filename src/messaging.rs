use std::{any::Any, fmt::Pointer, marker::PhantomData, time::Duration};

use derive_more::{Display, Error};
use futures::Future;

use crate::{
    actor::Actor,
    flow::MsgFlow,
    packets::{HandlerFn, Packet}, sending::TrySendError,
};

/// A [Req], which has been sent returns a [Reply], this can be awaited
/// to return the reply to the [Req].
#[derive(Debug)]
pub struct Reply<T> {
    receiver: oneshot::Receiver<T>,
}

impl<T> Reply<T> {
    pub async fn recv(self) -> Result<T, oneshot::RecvError> {
        self.receiver.await
    }
}

// unsafe impl<'a, A: Actor, P, R> Send for Req<'a, A, P, R> {}
// unsafe impl<'a, A: Actor, P, R> Sync for Req<'a, A, P, R> {}

/// The sender part of the [Req]. Is not allowed in public interface
pub(crate) struct InnerRequest<T> {
    sender: oneshot::Sender<T>,
}

impl<T> InnerRequest<T> {
    pub fn reply(self, reply: T) {
        let _ = self.sender.send(reply);
    }

    pub fn new() -> (Self, Reply<T>) {
        let (tx, rx) = oneshot::channel();

        let req = InnerRequest { sender: tx };
        let reply = Reply { receiver: rx };

        (req, reply)
    }
}

/// A message which will return a [Reply]
#[derive(Debug)]
pub struct Req<'a, A, P, R>
where
    A: Actor,
{
    packet: Packet<A>,
    sender: &'a PacketSender<A>,
    reply: Reply<R>,
    p: PhantomData<P>,
}

impl<'a, A, P, R> Req<'a, A, P, R>
where
    A: Actor,
{
    pub(crate) fn new(sender: &'a PacketSender<A>, packet: Packet<A>, reply: Reply<R>) -> Self
    where
        P: 'static,
        R: 'static,
    {
        Self {
            packet,
            sender,
            reply,
            p: PhantomData,
        }
    }

    pub fn send(self) -> Result<Reply<R>, TrySendError<P>>
    where
        P: 'static,
        R: 'static,
    {
        match self.sender.send(self.packet) {
            Ok(()) => Ok(self.reply),
            Err(e) => Err(TrySendError::ActorDied(e.0.get_params_req::<P, R>())),
        }
    }

    // pub fn try_send(self) -> Result<Reply<R>, TrySendError<P>>
    // where
    //     P: 'static,
    //     R: 'static,
    // {
    //     match self.sender.try_send(self.packet) {
    //         Ok(()) => Ok(self.reply),
    //         Err(e) => match e {
    //             PacketTrySendError::ActorDied(packet) => {
    //                 Err(TrySendError::ActorDied(packet.get_params_req::<P, R>()))
    //             }
    //             PacketTrySendError::Full(packet) => {
    //                 Err(TrySendError::Full(packet.get_params_req::<P, R>()))
    //             }
    //         },
    //     }
    // }
}



/// A message which does not return anything
#[derive(Debug)]
pub struct Msg<'a, A, P>
where
    A: Actor,
{
    packet: Packet<A>,
    sender: &'a PacketSender<A>,
    p: PhantomData<P>,
}

impl<'a, A, P> Msg<'a, A, P>
where
    A: Actor,
{
    pub(crate) fn new(sender: &'a PacketSender<A>, packet: Packet<A>) -> Self {
        Self {
            packet,
            sender,
            p: PhantomData,
        }
    }

    pub fn send(self) -> Result<(), TrySendError<P>>
    where
        P: 'static,
    {
        match self.sender.send(self.packet) {
            Ok(()) => Ok(()),
            Err(e) => Err(TrySendError::ActorDied(e.0.get_params_msg())),
        }
    }

    // pub fn try_send(self) -> Result<(), TrySendError<P>>
    // where
    //     P: 'static,
    // {
    //     match self.sender.try_send(self.packet) {
    //         Ok(()) => Ok(()),
    //         Err(e) => match e {
    //             PacketTrySendError::ActorDied(packet) => {
    //                 Err(TrySendError::ActorDied(packet.get_params_msg()))
    //             }
    //             PacketTrySendError::Full(packet) => {
    //                 Err(TrySendError::Full(packet.get_params_msg()))
    //             }
    //         },
    //     }
    // }
}

/// A sender of [Packet]s
#[derive(Debug)]
pub(crate) struct PacketSender<A: Actor + ?Sized> {
    sender: tokio::sync::mpsc::UnboundedSender<Packet<A>>,
}

impl<A: Actor> PacketSender<A> {
    pub fn send(&self, packet: Packet<A>) -> Result<(), PacketSendError<A>> {
        match self.sender.send(packet) {
            Ok(()) => Ok(()),
            Err(e) => Err(PacketSendError(e.0)),
        }
    }

    // pub fn try_send(&self, packet: Packet<A>) -> Result<(), PacketTrySendError<A>> {
    //     match self.sender.try_send(packet) {
    //         Ok(_) => Ok(()),
    //         Err(e) => match e {
    //             flume::TrySendError::Full(packet) => Err(PacketTrySendError::Full(packet)),
    //             flume::TrySendError::Disconnected(packet) => {
    //                 Err(PacketTrySendError::ActorDied(packet))
    //             }
    //         },
    //     }
    // }
}

pub(crate) struct PacketSendError<A: Actor>(pub Packet<A>);
pub(crate) enum PacketTrySendError<A: Actor> {
    ActorDied(Packet<A>),
    Full(Packet<A>),
}

impl<A: Actor> Clone for PacketSender<A> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

/// A receiver of [Packet]s
#[derive(Debug)]
pub(crate) struct PacketReceiver<A: Actor> {
    pub receiver: tokio::sync::mpsc::UnboundedReceiver<Packet<A>>,
}

unsafe impl<A: Actor> Send for PacketReceiver<A> {}
unsafe impl<A: Actor> Sync for PacketReceiver<A> {}

impl<A: Actor> PacketSender<A> {
    pub fn new() -> (PacketSender<A>, PacketReceiver<A>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        // tokio::sync::mpsc::channel();

        let tx = PacketSender { sender: tx };
        let rx = PacketReceiver { receiver: rx };

        (tx, rx)
    }
}

impl<A: Actor> PacketReceiver<A> {
    pub async fn recv(&mut self) -> Option<Packet<A>> {
        self.receiver.recv().await
    }
}

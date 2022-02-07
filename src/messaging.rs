use std::{any::Any, fmt::Pointer, marker::PhantomData, task::Poll, time::Duration};

use derive_more::{Display, Error};
use futures::{Future, FutureExt};

use crate::{
    actor::Actor,
    errors::{
        ActorDied, ActorDiedAfterSending, SendRecvError, TryRecvError, TrySendError,
        TrySendRecvError,
    },
    flows::Flow,
    packets::{Bounded, HandlerFn, InboxType, IsBounded, IsUnbounded, Packet, Unbounded},
};

/// A [Req], which has been sent returns a [Reply], this can be awaited
/// to return the reply to a [Req].
#[derive(Debug)]
pub struct Reply<T> {
    receiver: oneshot::Receiver<T>,
}

impl<T> Reply<T> {
    pub async fn async_recv(self) -> Result<T, ActorDiedAfterSending> {
        match self.receiver.await {
            Ok(t) => Ok(t),
            Err(e) => Err(ActorDiedAfterSending),
        }
    }

    pub fn try_recv(self) -> Result<T, TryRecvError> {
        self.receiver.try_recv().map_err(|e| match e {
            oneshot::TryRecvError::Empty => TryRecvError::NoReplyYet,
            oneshot::TryRecvError::Disconnected => TryRecvError::ActorDiedAfterSending,
        })
    }

    pub fn blocking_recv(self) -> Result<T, ActorDiedAfterSending> {
        self.receiver.recv().map_err(|_| ActorDiedAfterSending)
    }
}

impl<T> Future for Reply<T> {
    type Output = Result<T, ActorDiedAfterSending>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        self.receiver.poll_unpin(cx).map(|ready| match ready {
            Ok(t) => Ok(t),
            Err(e) => Err(ActorDiedAfterSending),
        })
    }
}

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

    pub fn send(self) -> Result<Reply<R>, ActorDied<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsUnbounded,
    {
        match self.sender.try_send_packet(self.packet) {
            Ok(()) => Ok(self.reply),
            Err(PacketTrySendError::Disconnected(packet)) => {
                Err(ActorDied(packet.get_params_req::<P, R>()))
            }
            Err(PacketTrySendError::Full(_)) => unreachable!("should be unbounded"),
        }
    }

    pub async fn send_recv(self) -> Result<R, SendRecvError<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsUnbounded,
    {
        match self.sender.try_send_packet(self.packet) {
            Ok(()) => Ok(self.reply.await?),
            Err(PacketTrySendError::Disconnected(packet)) => {
                Err(SendRecvError::ActorDied(packet.get_params_req::<P, R>()))
            }
            Err(PacketTrySendError::Full(packet)) => unreachable!("should be unbounded"),
        }
    }

    pub fn try_send(self) -> Result<Reply<R>, TrySendError<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.try_send_packet(self.packet) {
            Ok(()) => Ok(self.reply),
            Err(e) => match e {
                PacketTrySendError::Disconnected(packet) => {
                    Err(TrySendError::ActorDied(packet.get_params_req::<P, R>()))
                }
                PacketTrySendError::Full(packet) => {
                    Err(TrySendError::NoSpace(packet.get_params_req::<P, R>()))
                }
            },
        }
    }

    pub async fn try_send_recv(self) -> Result<R, TrySendRecvError<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.try_send_packet(self.packet) {
            Ok(()) => Ok(self.reply.await?),
            Err(e) => match e {
                PacketTrySendError::Disconnected(packet) => {
                    Err(TrySendRecvError::ActorDied(packet.get_params_req::<P, R>()))
                }
                PacketTrySendError::Full(packet) => {
                    Err(TrySendRecvError::NoSpace(packet.get_params_req::<P, R>()))
                }
            },
        }
    }

    pub async fn async_send(self) -> Result<Reply<R>, ActorDied<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.send_packet_async(self.packet).await {
            Ok(()) => Ok(self.reply),
            Err(PacketSendError::Disconnected(packet)) => {
                Err(ActorDied(packet.get_params_req::<P, R>()))
            }
        }
    }

    pub fn blocking_send(self) -> Result<Reply<R>, ActorDied<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.send_packet_blocking(self.packet) {
            Ok(()) => Ok(self.reply),
            Err(PacketSendError::Disconnected(packet)) => {
                Err(ActorDied(packet.get_params_req::<P, R>()))
            }
        }
    }

    pub async fn async_send_recv(self) -> Result<R, SendRecvError<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.send_packet_async(self.packet).await {
            Ok(()) => Ok(self.reply.await?),
            Err(PacketSendError::Disconnected(packet)) => {
                Err(SendRecvError::ActorDied(packet.get_params_req::<P, R>()))
            }
        }
    }

    pub fn blocking_send_blocking_recv(self) -> Result<R, SendRecvError<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.send_packet_blocking(self.packet) {
            Ok(()) => Ok(self.reply.blocking_recv()?),
            Err(PacketSendError::Disconnected(packet)) => {
                Err(SendRecvError::ActorDied(packet.get_params_req::<P, R>()))
            }
        }
    }

    pub fn try_send_blocking_recv(self) -> Result<R, TrySendRecvError<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.try_send_packet(self.packet) {
            Ok(()) => Ok(self.reply.blocking_recv()?),
            Err(e) => match e {
                PacketTrySendError::Disconnected(packet) => {
                    Err(TrySendRecvError::ActorDied(packet.get_params_req::<P, R>()))
                }
                PacketTrySendError::Full(packet) => {
                    Err(TrySendRecvError::NoSpace(packet.get_params_req::<P, R>()))
                }
            },
        }
    }
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

    pub fn send(self) -> Result<(), ActorDied<P>>
    where
        P: 'static,
        A::Inbox: IsUnbounded,
    {
        match self.sender.try_send_packet(self.packet) {
            Ok(()) => Ok(()),
            Err(PacketTrySendError::Disconnected(packet)) => {
                Err(ActorDied(packet.get_params_msg::<P>()))
            }
            Err(PacketTrySendError::Full(_)) => unreachable!("should be unbounded"),
        }
    }

    pub fn try_send(self) -> Result<(), TrySendError<P>>
    where
        P: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.try_send_packet(self.packet) {
            Ok(()) => Ok(()),
            Err(e) => match e {
                PacketTrySendError::Disconnected(packet) => {
                    Err(TrySendError::ActorDied(packet.get_params_msg()))
                }
                PacketTrySendError::Full(packet) => {
                    Err(TrySendError::NoSpace(packet.get_params_msg()))
                }
            },
        }
    }

    pub async fn async_send(self) -> Result<(), ActorDied<P>>
    where
        P: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.send_packet_async(self.packet).await {
            Ok(()) => Ok(()),
            Err(PacketSendError::Disconnected(packet)) => Err(ActorDied(packet.get_params_msg())),
        }
    }

    pub fn blocking_send(self) -> Result<(), ActorDied<P>>
    where
        P: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.send_packet_blocking(self.packet) {
            Ok(()) => Ok(()),
            Err(PacketSendError::Disconnected(packet)) => Err(ActorDied(packet.get_params_msg())),
        }
    }
}

/// A sender of [Packet]s
#[derive(Debug)]
pub struct PacketSender<A: Actor + ?Sized> {
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

        // if !self.is_alive() {
        //     Err(PacketTrySendError::Disconnected(packet))
        // } else {
        //     res
        // }
    }

    pub(crate) async fn send_packet_async(
        &self,
        packet: Packet<A>,
    ) -> Result<(), PacketSendError<A>> {
        // if !self.is_alive() {
        //     return Err(PacketSendError::Disconnected(packet))
        // }
        match self.sender.send(packet).await {
            Ok(()) => Ok(()),
            Err(e) => Err(PacketSendError::Disconnected(e.0)),
        }
    }

    pub(crate) fn send_packet_blocking(&self, packet: Packet<A>) -> Result<(), PacketSendError<A>> {
        // if !self.is_alive() {
        //     return Err(PacketSendError::Disconnected(packet))
        // }
        todo!()
        // match self.sender.try_send(packet) {
        //     Ok(()) => Ok(()),
        //     Err(e) => Err(PacketSendError::Disconnected(e)),
        // }
    }
}

pub(crate) enum PacketTrySendError<A: Actor> {
    Full(Packet<A>),
    Disconnected(Packet<A>),
}

pub(crate) enum PacketSendError<A: Actor> {
    Disconnected(Packet<A>),
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
pub struct PacketReceiver<A: Actor> {
    pub receiver: async_channel::Receiver<Packet<A>>,
}

pub trait NewInbox<A: Actor, B: InboxType>: InboxType {
    fn new_packet_sender(cap: usize) -> (PacketSender<A>, PacketReceiver<A>);
}

impl<A: Actor> NewInbox<A, Bounded> for Bounded {
    fn new_packet_sender(cap: usize) -> (PacketSender<A>, PacketReceiver<A>) {
        PacketSender::new(Some(cap))
    }
}

impl<A: Actor> NewInbox<A, Unbounded> for Unbounded {
    fn new_packet_sender(_cap: usize) -> (PacketSender<A>, PacketReceiver<A>) {
        PacketSender::new(None)
    }
}

impl<A: Actor> PacketSender<A> {
    fn new(size: Option<usize>) -> (PacketSender<A>, PacketReceiver<A>) {
        let (tx, rx) = match size {
            Some(size) => async_channel::bounded(size),
            None => async_channel::unbounded(),
        };

        // tokio::sync::mpsc::channel();

        let tx = PacketSender { sender: tx };
        let rx = PacketReceiver { receiver: rx };

        (tx, rx)
    }
}

impl<A: Actor> PacketReceiver<A> {
    pub async fn recv(&mut self) -> Option<Packet<A>> {
        self.receiver.recv().await.ok()
    }
}

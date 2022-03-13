use crate::{
    action::Action,
    actor::Actor,
    errors::{DidntArrive, NoReply, ReqRecvError, RequestDropped, TryRecvError},
    inbox::{ActionSender, ActionTrySendError, Inbox},
};
use futures::{Future, FutureExt};
use std::{marker::PhantomData, task::Poll};

//--------------------------------------------------------------------------------------------------
//  Request
//--------------------------------------------------------------------------------------------------

/// The sender part of the [Req]. Is not allowed in public interface
#[derive(Debug)]
pub struct Request<T> {
    sender: oneshot::Sender<T>,
}

impl<T> Request<T> {
    /// Send back a reply.
    pub fn reply(self, reply: T) -> Result<(), RequestDropped<T>> {
        self.sender.send(reply).map_err(|e| RequestDropped(e.into_inner()))
    }

    /// Create a new request.
    pub fn new() -> (Self, Reply<T>) {
        let (tx, rx) = oneshot::channel();

        let req = Request { sender: tx };
        let reply = Reply { receiver: rx };

        (req, reply)
    }
}

//--------------------------------------------------------------------------------------------------
//  Reply
//--------------------------------------------------------------------------------------------------

/// When a [Req] is sent, you get a [Reply]. this can be awaited to return the reply to this [Req].
#[derive(Debug)]
#[must_use]
pub struct Reply<T> {
    receiver: oneshot::Receiver<T>,
}

impl<T> Reply<T> {
    /// Try if the [Reply] is ready. Can fail either if the [Reply] is not yet ready, or if the
    /// [Actor] died.
    pub fn try_recv(self) -> Result<T, TryRecvError> {
        self.receiver.try_recv().map_err(|e| match e {
            oneshot::TryRecvError::Empty => TryRecvError::NoReplyYet,
            oneshot::TryRecvError::Disconnected => TryRecvError::NoReply,
        })
    }

    /// Wait synchronously for this reply. Can fail only if the [Actor] dies.
    pub fn recv_blocking(self) -> Result<T, NoReply> {
        self.receiver.recv().map_err(|_| NoReply)
    }
}

impl<T> Future for Reply<T> {
    type Output = Result<T, NoReply>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        self.receiver.poll_unpin(cx).map(|ready| match ready {
            Ok(t) => Ok(t),
            Err(_e) => Err(NoReply),
        })
    }
}

//--------------------------------------------------------------------------------------------------
//  Req
//--------------------------------------------------------------------------------------------------

/// A request that can be sent. If this is sent, a [Reply] will be returned. This reply can then be
/// `await`ed to return the actual reply.
#[derive(Debug)]
#[must_use]
pub(crate) struct Req<'a, A, P, R>
where
    A: Actor,
{
    action: Action<A>,
    sender: &'a ActionSender<A>,
    reply: Reply<R>,
    p: PhantomData<P>,
}

impl<'a, A, P, R> Req<'a, A, P, R>
where
    A: Actor,
{
    /// create a new request.
    pub(crate) fn new(sender: &'a ActionSender<A>, action: Action<A>, reply: Reply<R>) -> Self
    where
        // P: 'static,
        // R: 'static,
    {
        Self {
            action,
            sender,
            reply,
            p: PhantomData,
        }
    }

    /// If the [Actor::Inbox] is [Unbounded] (default), then a message can be sent without
    /// waiting for space in the inbox. For this reason, [Unbounded] mailboxes only have a single
    /// `send` method, which works for both synchronous and asynchronous contexts.
    ///
    /// This method can only fail if the [Actor] has died before the message could be sent.
    pub fn send(self) -> Result<Reply<R>, DidntArrive<P>>
    where
        P: Send + 'static,
        R: Send + 'static,
    {
        match self.sender.try_send(self.action) {
            Ok(()) => Ok(self.reply),
            Err(ActionTrySendError::Disconnected(action)) => {
                Err(DidntArrive(unsafe { action.transmute_req_ref::<P, R>().0 }))
            }
            Err(ActionTrySendError::Full(_)) => unreachable!("should be unbounded"),
        }
    }

        /// If the [Actor::Inbox] is [Unbounded] (default), then a message can be sent without
    /// waiting for space in the inbox. For this reason, [Unbounded] mailboxes only have a single
    /// `send` method, which works for both synchronous and asynchronous contexts.
    ///
    /// This method can only fail if the [Actor] has died before the message could be sent.
    pub unsafe fn send_ref(self) -> Result<Reply<R>, DidntArrive<P>>
    where
        P: Send + 'a,
        R: Send + 'a,
    {
        match self.sender.try_send(self.action) {
            Ok(()) => Ok(self.reply),
            Err(ActionTrySendError::Disconnected(action)) => {
                Err(DidntArrive(action.transmute_req_ref::<P, R>().0))
            }
            Err(ActionTrySendError::Full(_)) => unreachable!("should be unbounded"),
        }
    }

    /// A method to make sending ([Unbounded]) and then asynchronously receiving easier. It
    /// combines `.send()` and `.recv_async()`, and unifies the error types that could be
    /// returned.
    ///
    /// This method can fail if the [Actor] dies before the method could be sent, or before a
    /// reply was received.
    pub async fn send_recv(self) -> Result<R, ReqRecvError<P>>
    where
        P: Send + 'static,
        R: Send + 'static,
    {
        match self.sender.try_send(self.action) {
            Ok(()) => Ok(self.reply.await?),
            Err(ActionTrySendError::Disconnected(action)) => Err(ReqRecvError::DidntArrive(
                unsafe { action.transmute_req_ref::<P, R>().0 },
            )),
            Err(ActionTrySendError::Full(_action)) => unreachable!("should be unbounded"),
        }
    }

    /// A method to make `send` followed by `blocking_recv` easier. It sends the
    /// message, and if this succeeds synchronously awaits the reply. It combines these error
    /// types into a single unified error.
    ///
    /// This method can fail if either the [Actor] dies before sending or if it dies before a
    /// [Reply] could be received.
    pub fn send_blocking_recv(self) -> Result<R, ReqRecvError<P>>
    where
        P: Send + 'static,
        R: Send + 'static,
    {
        match self.sender.try_send(self.action) {
            Ok(()) => Ok(self.reply.recv_blocking()?),
            Err(e) => match e {
                ActionTrySendError::Disconnected(action) => Err(ReqRecvError::DidntArrive(
                    unsafe { action.transmute_req_ref::<P, R>().0 },
                )),
                ActionTrySendError::Full(_action) => {
                    unreachable!("Should be unbounded!")
                }
            },
        }
    }
}

//--------------------------------------------------------------------------------------------------
//  Msg
//--------------------------------------------------------------------------------------------------

/// A message that can be sent. This will not return a [Reply].
#[derive(Debug)]
#[must_use]
pub(crate) struct Msg<'a, A, P>
where
    A: Actor,
{
    action: Action<A>,
    sender: &'a ActionSender<A>,
    p: PhantomData<P>,
}

impl<'a, A, P> Msg<'a, A, P>
where
    A: Actor,
{
    /// Create a new message.
    pub(crate) fn new(sender: &'a ActionSender<A>, action: Action<A>) -> Self {
        Self {
            action,
            sender,
            p: PhantomData,
        }
    }

    /// If the [Actor::Inbox] is [Unbounded] (default), then a message can be sent without
    /// waiting for space in the inbox. For this reason, [Unbounded] mailboxes only have a single
    /// `send` method, which works for both synchronous and asynchronous contexts.
    ///
    /// This method can only fail if the [Actor] has died before the message could be sent.
    pub fn send(self) -> Result<(), DidntArrive<P>>
    where
        P: Send + 'static,
    {
        match self.sender.try_send(self.action) {
            Ok(()) => Ok(()),
            Err(ActionTrySendError::Disconnected(action)) => {
                Err(DidntArrive(action.downcast::<P>().unwrap()))
            }
            Err(ActionTrySendError::Full(_)) => unreachable!("should be unbounded"),
        }
    }
}

impl<A: Actor> Unpin for Inbox<A> {}

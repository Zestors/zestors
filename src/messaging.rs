use crate::{
    action::Action,
    actor::Actor,
    errors::{
        ActorDied, ActorDiedAfterSending, SendRecvError, TryRecvError, TrySendError,
        TrySendRecvError,
    },
    inbox::{
        ActionReceiver, ActionSendError, ActionSender, ActionTrySendError, IsBounded, IsUnbounded,
    },
};
use futures::{Future, FutureExt};
use std::{marker::PhantomData, task::Poll};

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
    /// Wait asynchronously for this [Reply]. Can fail only if the [Actor] dies.
    ///
    /// This is the same as directly `.await`ing the [Reply].
    pub async fn async_recv(self) -> Result<T, ActorDiedAfterSending> {
        self.await
    }

    /// Try if the [Reply] is ready. Can fail either if the [Reply] is not yet ready, or if the
    /// [Actor] died.
    pub fn try_recv(self) -> Result<T, TryRecvError> {
        self.receiver.try_recv().map_err(|e| match e {
            oneshot::TryRecvError::Empty => TryRecvError::NoReplyYet,
            oneshot::TryRecvError::Disconnected => TryRecvError::ActorDiedAfterSending,
        })
    }

    /// Wait synchronously for this reply. Can fail only if the [Actor] dies.
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
            Err(_e) => Err(ActorDiedAfterSending),
        })
    }
}

//--------------------------------------------------------------------------------------------------
//  InnerRequest
//--------------------------------------------------------------------------------------------------

/// The sender part of the [Req]. Is not allowed in public interface
pub(crate) struct InternalRequest<T> {
    sender: oneshot::Sender<T>,
}

impl<T> InternalRequest<T> {
    /// Send back a reply.
    pub(crate) fn reply(self, reply: T) {
        let _ = self.sender.send(reply);
    }

    /// Create a new request.
    pub(crate) fn new() -> (Self, Reply<T>) {
        let (tx, rx) = oneshot::channel();

        let req = InternalRequest { sender: tx };
        let reply = Reply { receiver: rx };

        (req, reply)
    }
}

//--------------------------------------------------------------------------------------------------
//  Req
//--------------------------------------------------------------------------------------------------

/// A request that can be sent. If this is sent, a [Reply] will be returned. This reply can then be
/// `await`ed to return the actual reply.
#[derive(Debug)]
#[must_use]
pub struct Req<'a, A, P, R>
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
        P: 'static,
        R: 'static,
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
    pub fn send(self) -> Result<Reply<R>, ActorDied<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsUnbounded,
    {
        match self.sender.try_send(self.action) {
            Ok(()) => Ok(self.reply),
            Err(ActionTrySendError::Disconnected(action)) => {
                Err(ActorDied(action.get_params_req::<P, R>().unwrap()))
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
    pub async fn send_recv(self) -> Result<R, SendRecvError<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsUnbounded,
    {
        match self.sender.try_send(self.action) {
            Ok(()) => Ok(self.reply.await?),
            Err(ActionTrySendError::Disconnected(action)) => {
                Err(SendRecvError::ActorDied(action.get_params_req::<P, R>().unwrap()))
            }
            Err(ActionTrySendError::Full(_action)) => unreachable!("should be unbounded"),
        }
    }

    /// A method to make `send` followed by `blocking_recv` easier. It sends the
    /// message, and if this succeeds synchronously awaits the reply. It combines these error
    /// types into a single unified error.
    ///
    /// This method can fail if either the [Actor] dies before sending or if it dies before a
    /// [Reply] could be received.
    pub fn send_blocking_recv(self) -> Result<R, SendRecvError<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsUnbounded,
    {
        match self.sender.try_send(self.action) {
            Ok(()) => Ok(self.reply.blocking_recv()?),
            Err(e) => match e {
                ActionTrySendError::Disconnected(action) => {
                    Err(SendRecvError::ActorDied(action.get_params_req::<P, R>().unwrap()))
                }
                ActionTrySendError::Full(_action) => {
                    unreachable!("Should be unbounded!")
                }
            },
        }
    }

    /// If the [Actor::Inbox] is [Bounded], then it is not guaranteed that the inbox has space.
    /// This method fails to send if the inbox is currently full or if the [Actor] has died.
    pub fn try_send(self) -> Result<Reply<R>, TrySendError<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.try_send(self.action) {
            Ok(()) => Ok(self.reply),
            Err(e) => match e {
                ActionTrySendError::Disconnected(action) => {
                    Err(TrySendError::ActorDied(action.get_params_req::<P, R>().unwrap()))
                }
                ActionTrySendError::Full(action) => {
                    Err(TrySendError::NoSpace(action.get_params_req::<P, R>().unwrap()))
                }
            },
        }
    }

    /// A method to make `try_send` followed by `recv_async` easier. It first tries to send the
    /// message, and if this succeeds asynchronously awaits the reply. It combines these error
    /// types into a single unified error.
    ///
    /// This method can fail if either the [Actor] dies before sending, if it dies before a
    /// [Reply] could be received or if the inbox is full.
    pub async fn try_send_recv(self) -> Result<R, TrySendRecvError<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.try_send(self.action) {
            Ok(()) => Ok(self.reply.await?),
            Err(e) => match e {
                ActionTrySendError::Disconnected(action) => {
                    Err(TrySendRecvError::ActorDied(action.get_params_req::<P, R>().unwrap()))
                }
                ActionTrySendError::Full(action) => {
                    Err(TrySendRecvError::NoSpace(action.get_params_req::<P, R>().unwrap()))
                }
            },
        }
    }

    /// If the [Actor::Inbox] is [Bbounded], then it is not guaranteed the inbox has
    /// space. Therefore to guarantee the message arrives, it must sometimes wait for space to be
    /// free. This method waits asynchronously until there is space in the inbox and then sends
    /// the message.
    ///
    /// This method can fail only if the [Actor] dies.
    pub async fn async_send(self) -> Result<Reply<R>, ActorDied<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.send_async(self.action).await {
            Ok(()) => Ok(self.reply),
            Err(ActionSendError::Disconnected(action)) => {
                Err(ActorDied(action.get_params_req::<P, R>().unwrap()))
            }
        }
    }

    /// A method to make `async_send` followed by `recv_async` easier. It first sends the
    /// message, and if this succeeds asynchronously awaits the reply. It combines these error
    /// types into a single unified error.
    ///
    /// This method can fail if either the [Actor] dies before sending or if it dies before a
    /// [Reply] could be received.
    pub async fn async_send_recv(self) -> Result<R, SendRecvError<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.send_async(self.action).await {
            Ok(()) => Ok(self.reply.await?),
            Err(ActionSendError::Disconnected(action)) => {
                Err(SendRecvError::ActorDied(action.get_params_req::<P, R>().unwrap()))
            }
        }
    }

    /// A method to make `try_send` followed by `blocking_recv` easier. It first tries to send the
    /// message, and if this succeeds synchronously awaits the reply. It combines these error
    /// types into a single unified error.
    ///
    /// This method can fail if either the [Actor] dies before sending or if it dies before a
    /// [Reply] could be received.
    pub fn try_send_blocking_recv(self) -> Result<R, TrySendRecvError<P>>
    where
        P: 'static,
        R: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.try_send(self.action) {
            Ok(()) => Ok(self.reply.blocking_recv()?),
            Err(e) => match e {
                ActionTrySendError::Disconnected(action) => {
                    Err(TrySendRecvError::ActorDied(action.get_params_req::<P, R>().unwrap()))
                }
                ActionTrySendError::Full(action) => {
                    Err(TrySendRecvError::NoSpace(action.get_params_req::<P, R>().unwrap()))
                }
            },
        }
    }

    // /// Same as [Req::async_send] but blocking.
    // pub fn blocking_send(self) -> Result<Reply<R>, ActorDied<P>>
    // where
    //     P: 'static,
    //     R: 'static,
    //     A::Inbox: IsBounded,
    // {
    //     match self.sender.send_action_blocking(self.action) {
    //         Ok(()) => Ok(self.reply),
    //         Err(PacketSendError::Disconnected(action)) => {
    //             Err(ActorDied(action.get_params_req::<P, R>()))
    //         }
    //     }
    // }

    // pub fn blocking_send_blocking_recv(self) -> Result<R, SendRecvError<P>>
    // where
    //     P: 'static,
    //     R: 'static,
    //     A::Inbox: IsBounded,
    // {
    //     match self.sender.send_action_blocking(self.action) {
    //         Ok(()) => Ok(self.reply.blocking_recv()?),
    //         Err(PacketSendError::Disconnected(action)) => {
    //             Err(SendRecvError::ActorDied(action.get_params_req::<P, R>()))
    //         }
    //     }
    // }
}

//--------------------------------------------------------------------------------------------------
//  Msg
//--------------------------------------------------------------------------------------------------

/// A message that can be sent. This will not return a [Reply].
#[derive(Debug)]
#[must_use]
pub struct Msg<'a, A, P>
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
    pub fn send(self) -> Result<(), ActorDied<P>>
    where
        P: 'static,
        A::Inbox: IsUnbounded,
    {
        match self.sender.try_send(self.action) {
            Ok(()) => Ok(()),
            Err(ActionTrySendError::Disconnected(action)) => {
                Err(ActorDied(action.get_params::<P>().unwrap()))
            }
            Err(ActionTrySendError::Full(_)) => unreachable!("should be unbounded"),
        }
    }

    /// If the [Actor::Inbox] is [Bounded], then it is not guaranteed that the inbox has space.
    /// This method fails to send if the inbox is currently full or if the [Actor] has died.
    pub fn try_send(self) -> Result<(), TrySendError<P>>
    where
        P: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.try_send(self.action) {
            Ok(()) => Ok(()),
            Err(e) => match e {
                ActionTrySendError::Disconnected(action) => {
                    Err(TrySendError::ActorDied(action.get_params().unwrap()))
                }
                ActionTrySendError::Full(action) => {
                    Err(TrySendError::NoSpace(action.get_params().unwrap()))
                }
            },
        }
    }

    /// If the [Actor::Inbox] is [Bounded], then it is not guaranteed the inbox has
    /// space. Therefore to guarantee the message arrives, it must sometimes wait for space to be
    /// free. This method waits asynchronously until there is space in the inbox and then sends
    /// the message.
    ///
    /// This method can fail only if the [Actor] dies.
    pub async fn async_send(self) -> Result<(), ActorDied<P>>
    where
        P: 'static,
        A::Inbox: IsBounded,
    {
        match self.sender.send_async(self.action).await {
            Ok(()) => Ok(()),
            Err(ActionSendError::Disconnected(action)) => Err(ActorDied(action.get_params().unwrap())),
        }
    }

    // pub fn blocking_send(self) -> Result<(), ActorDied<P>>
    // where
    //     P: 'static,
    //     A::Inbox: IsBounded,
    // {
    //     match self.sender.send_action_blocking(self.action) {
    //         Ok(()) => Ok(()),
    //         Err(PacketSendError::Disconnected(action)) => Err(ActorDied(action.get_params())),
    //     }
    // }
}

impl<A: Actor> Unpin for ActionReceiver<A> {}

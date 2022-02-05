use std::pin::Pin;

use futures::Future;

use crate::{callable::{RemoteFunction, Callable}, actor::{Unbounded, IsUnbounded, Actor}, messaging::{Msg, Req, Reply}};
use std::fmt::Debug;

//-------------------------------------
// Errors
//-------------------------------------


#[derive(Debug)]
/// Sending failed because the actor is not alive
pub enum SendError<T> {
    ActorDied(T),
}

#[derive(Debug)]
/// Sending failed because actor is not alive, or because no reply will be sent back
enum SendRecvError<T> {
    ActorDied(T),
    NoReply
}

#[derive(Debug)]
/// Sending failed because the actor is not alive, or because it's inbox is full
pub enum BoundedSendError<T> {
    ActorDied(T),
    NoSpace(T),
}

#[derive(Debug)]
/// Sending has failed because the actor is not alive, because it's inbox is full,
/// or because no reply will be sent back
enum BoundedSendRecvError<T> {
    NoSpace(T),
    ActorDied(T),
    NoReply
}

#[derive(Debug)]
/// Receiving has failed, because no reply will be sent back.
enum RecvError {
    NoReply
}

#[derive(Debug)]
/// Receiving has failed, because no reply will ever be sent back, or no reply has been
/// sent back as of yet.
enum TryRecvError<T> {
    NoReply,
    NoReplyYet(Reply<T>)
}

//-------------------------------------
// UnboundedSend
//-------------------------------------

pub trait UnboundedSend<'a, 'b, I, F, P, R, G, A, B> {
    fn send(&'a self, function: RemoteFunction<F>, params: P) -> Result<R, SendError<P>>;

    // fn send(
    //     &'a self,
    //     function: RemoteFunction<F>,
    //     params: P,
    // ) -> Pin<Box<dyn Future<Output = Result<R, SendError<P>>> + 'a>>;
}

impl<'a, 'b, I, F, P, G, A, T> UnboundedSend<'a, 'b, I, F, P, (), G, A, Unbounded> for T
where
    T: Callable<'a, 'b, I, F, P, (), G, Msg<'a, A, P>>,
    A::Inbox: IsUnbounded,
    A: Actor,
    P: Send + 'static,
    F: 'a,
{
    fn send(&'a self, function: RemoteFunction<F>, params: P) -> Result<(), SendError<P>> {
        self.call(function, params).send()
    }
}

impl<'a, 'b, I, F, P, G, R, A, T> UnboundedSend<'a, 'b, I, F, P, Reply<R>, G, A, Unbounded> for T
where
    T: Callable<'a, 'b, I, F, P, R, G, Req<'a, A, P, R>>,
    A::Inbox: IsUnbounded,
    A: Actor,
    P: Send + 'static,
    R: Send + 'static,
    F: 'a,
{
    fn send(&'a self, function: RemoteFunction<F>, params: P) -> Result<Reply<R>, SendError<P>> {
        self.call(function, params).send()
    }
}

//-------------------------------------
// UnboundedSendRecv
//-------------------------------------

pub trait UnboundedSendRecv<'a, 'b, I, F, P, R, G, A, B> {
    fn send_recv(
        &'a self,
        function: RemoteFunction<F>,
        params: P,
    ) -> Pin<Box<dyn Future<Output = Result<R, SendRecvError<P>>> + 'a>>;
}

impl<'a, 'b, I, F, P, G, A, T, R> UnboundedSendRecv<'a, 'b, I, F, P, R, G, A, Unbounded> for T
where
    T: Callable<'a, 'b, I, F, P, R, G, Req<'a, A, P, R>>,
    A::Inbox: IsUnbounded,
    A: Actor,
    P: Send + 'static + Debug,
    F: 'a,
{
    fn send_recv(
        &'a self,
        function: RemoteFunction<F>,
        params: P,
    ) -> Pin<Box<dyn Future<Output = Result<R, SendRecvError<P>>> + 'a>> {
        Box::pin(async move {
            self.call(function, params).send().unwrap().recv().await
        })
    }
}



impl<T> std::fmt::Display for TrySendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Error {}", "Full"))
    }
}

impl<T> std::fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("Error {}", "Full"))
    }
}

impl<T: std::fmt::Debug> std::error::Error for TrySendError<T> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl<T: std::fmt::Debug> std::error::Error for SendError<T> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}
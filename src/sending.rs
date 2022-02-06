use std::pin::Pin;

use derive_more::{Display, Error};
use futures::Future;

use crate::{
    actor::{Actor, IsBounded, IsUnbounded, Unbounded},
    callable::{Callable, RemoteFunction},
    messaging::{Msg, Reply, Req}, errors::{ActorDied, TrySendError},
};
use std::fmt::Debug;

//-------------------------------------
// UnboundedSend
//-------------------------------------

pub trait UnboundedSend<'a, 'b, I, F, P, R, G, A> {
    fn send(&'a self, function: RemoteFunction<F>, params: P) -> Result<R, ActorDied<P>>;
}

impl<'a, 'b, I, F, P, G, A, T> UnboundedSend<'a, 'b, I, F, P, (), G, A> for T
where
    T: Callable<'a, 'b, I, F, P, (), G, Msg<'a, A, P>>,
    A::Inbox: IsUnbounded,
    A: Actor,
    P: Send + 'static,
    F: 'a,
{
    fn send(&'a self, function: RemoteFunction<F>, params: P) -> Result<(), ActorDied<P>> {
        self.call(function, params).send()
    }
}

impl<'a, 'b, I, F, P, G, R, A, T> UnboundedSend<'a, 'b, I, F, P, Reply<R>, G, A> for T
where
    T: Callable<'a, 'b, I, F, P, R, G, Req<'a, A, P, R>>,
    A::Inbox: IsUnbounded,
    A: Actor,
    P: Send + 'static,
    R: Send + 'static,
    F: 'a,
{
    fn send(&'a self, function: RemoteFunction<F>, params: P) -> Result<Reply<R>, ActorDied<P>> {
        self.call(function, params).send()
    }
}

//-------------------------------------
// BoundedSend
//-------------------------------------

pub trait BoundedSend<'a, 'b, I, F, P, R, G, A> {
    fn try_send(&'a self, function: RemoteFunction<F>, params: P) -> Result<R, TrySendError<P>>;
    fn async_send(
        &'a self,
        function: RemoteFunction<F>,
        params: P,
    ) -> Pin<Box<dyn Future<Output = Result<R, ActorDied<P>>> + 'a>>;
    fn blocking_send(&'a self, function: RemoteFunction<F>, params: P) -> Result<R, ActorDied<P>>;
}

impl<'a, 'b, I, F, P, R, G, A, T> BoundedSend<'a, 'b, I, F, P, Reply<R>, G, A> for T
where
    T: Callable<'a, 'b, I, F, P, R, G, Req<'a, A, P, R>>,
    A::Inbox: IsBounded,
    A: Actor,
    P: Send + 'static,
    R: Send + 'static,
    F: 'a,
{
    fn try_send(
        &'a self,
        function: RemoteFunction<F>,
        params: P,
    ) -> Result<Reply<R>, TrySendError<P>> {
        self.call(function, params).try_send()
    }

    fn async_send(
        &'a self,
        function: RemoteFunction<F>,
        params: P,
    ) -> Pin<Box<dyn Future<Output = Result<Reply<R>, ActorDied<P>>> + 'a>> {
        Box::pin(async move { self.call(function, params).async_send().await })
    }

    fn blocking_send(
        &'a self,
        function: RemoteFunction<F>,
        params: P,
    ) -> Result<Reply<R>, ActorDied<P>> {
        self.call(function, params).blocking_send()
    }
}

impl<'a, 'b, I, F, P, G, A, T> BoundedSend<'a, 'b, I, F, P, (), G, A> for T
where
    T: Callable<'a, 'b, I, F, P, (), G, Msg<'a, A, P>>,
    A::Inbox: IsBounded,
    A: Actor,
    P: Send + 'static,
    F: 'a,
{
    fn try_send(&'a self, function: RemoteFunction<F>, params: P) -> Result<(), TrySendError<P>> {
        self.call(function, params).try_send()
    }

    fn async_send(
        &'a self,
        function: RemoteFunction<F>,
        params: P,
    ) -> Pin<Box<dyn Future<Output = Result<(), ActorDied<P>>> + 'a>> {
        Box::pin(async move { self.call(function, params).async_send().await })
    }

    fn blocking_send(&'a self, function: RemoteFunction<F>, params: P) -> Result<(), ActorDied<P>> {
        self.call(function, params).blocking_send()
    }
}




// //-------------------------------------
// // UnboundedSendRecv
// //-------------------------------------

// pub trait UnboundedSendRecv<'a, 'b, I, F, P, R, G, A> {
//     fn send_recv(
//         &'a self,
//         function: RemoteFunction<F>,
//         params: P,
//     ) -> Pin<Box<dyn Future<Output = Result<R, SendRecvError<P>>> + 'a>>;
// }

// // unbounded send_recv for req
// impl<'a, 'b, I, F, P, G, A, T, R> UnboundedSendRecv<'a, 'b, I, F, P, R, G, A> for T
// where
//     T: Callable<'a, 'b, I, F, P, R, G, Req<'a, A, P, R>>,
//     A::Inbox: IsUnbounded,
//     A: Actor,
//     P: Send + 'static,
//     R: Send + 'static,
//     F: 'a,
// {
//     fn send_recv(
//         &'a self,
//         function: RemoteFunction<F>,
//         params: P,
//     ) -> Pin<Box<dyn Future<Output = Result<R, SendRecvError<P>>> + 'a>> {
//         Box::pin(async move { self.send_recv(function, params).await })
//     }
// }

// //-------------------------------------
// // BoundedSendRecv
// //-------------------------------------

// pub trait BoundedSendRecv<'a, 'b, I, F, P, R, G, A> {
//     fn try_send_recv(
//         &'a self,
//         function: RemoteFunction<F>,
//         params: P,
//     ) -> Result<R, TrySendRecvError<P>>;

//     fn async_send_recv(
//         &'a self,
//         function: RemoteFunction<F>,
//         params: P,
//     ) -> Result<R, SendRecvError<P>>;

//     fn try_send_blocking_recv(
//         &'a self,
//         function: RemoteFunction<F>,
//         params: P,
//     ) -> Result<R, TrySendRecvError<P>>;

//     fn blocking_send_blocking_recv(
//         &'a self,
//         function: RemoteFunction<F>,
//         params: P,
//     ) -> Result<R, SendRecvError<P>>;
// }


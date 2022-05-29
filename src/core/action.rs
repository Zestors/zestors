use futures::{Future, Stream};

use crate::core::*;
use crate::Fn;
use std::{any::Any, error::Error, marker::PhantomData, mem::transmute, pin::Pin};

//------------------------------------------------------------------------------------------------
//  Action!
//------------------------------------------------------------------------------------------------

#[macro_export]
macro_rules! Action {
    ($function:expr, $param:expr) => {
        $crate::core::action::Action::new($crate::Fn!($function), $param)
    };
}

//------------------------------------------------------------------------------------------------
//  Action
//------------------------------------------------------------------------------------------------

/// An `Action` is one of the central primitives within zestors.
///
/// `Actor`s constantly run an event-loop, waiting for new `Action`s to execute. `Action`s can be
/// sent as messages, but they can also be created by the `Actor` itself and scheduled on it's
/// event-loop.
///
/// `Action`s can be created in two ways:
/// 1. Through `Action::new(Fn!(MyActor::handle_message), params)`;
/// 2. Through `Action!(MyActor::handle_message, params)`;
///
/// When sending a message, it is not necessary to create the `Action` manually, but instead you
/// can just use `addr.call(Fn!(MyActor::handle_fn), params)` directly. It is also possible to
/// manually use `addr.send(Action!(MyActor::handle_fn, params))`.
#[derive(Debug)]
pub struct Action<A: ?Sized> {
    phantom_data: PhantomData<A>,
    handler_fn: UntypedHandlerFn,
    params: Box<dyn Any + Send>,
}

impl<A> Action<A> {
    pub async fn handle(self, actor: &mut A, state: &mut State<A>) -> HandlerOutput<A>
    where
        A: Actor,
    {
        unsafe { self.handler_fn.call(actor, state, self.params).await }
    }

    pub fn new<M, R>(actor_fn: HandlerFn<A, Snd<M>, R>, msg: M) -> (Self, R)
    where
        M: Send + 'static,
        R: RcvPart,
    {
        R::new_action(actor_fn, msg)
    }

    pub fn downcast<M: 'static, R: RcvPart + 'static>(self) -> Result<(M, R::SndPart), Self> {
        match self.params.downcast::<(M, R::SndPart)>() {
            Ok(msg) => Ok(*msg),
            Err(params) => Err(Self {
                phantom_data: self.phantom_data,
                handler_fn: self.handler_fn,
                params,
            }),
        }
    }
}

//------------------------------------------------------------------------------------------------
//  IntoAction
//------------------------------------------------------------------------------------------------

impl<A, M: Send + 'static, R: RcvPart> HandlerFn<A, Snd<M>, R> {
    fn into_action(self, msg: M) -> (Action<A>, R) {
        R::new_action(self, msg)
    }
}

pub trait RcvPart: Sized + Send + 'static {
    type SndPart;

    fn new_action<A, M>(fun: HandlerFn<A, Snd<M>, Self>, msg: M) -> (Action<A>, Self)
    where
        M: Send + 'static;
}

impl RcvPart for () {
    type SndPart = ();

    fn new_action<A, M>(fun: HandlerFn<A, Snd<M>, Self>, msg: M) -> (Action<A>, Self)
    where
        M: Send + 'static,
    {
        (
            Action {
                phantom_data: PhantomData,
                handler_fn: fun.into_any(),
                params: Box::new((msg, ())),
            },
            (),
        )
    }
}

impl<R: Send + 'static> RcvPart for Rcv<R> {
    type SndPart = Snd<R>;

    fn new_action<A, M>(fun: HandlerFn<A, Snd<M>, Self>, msg: M) -> (Action<A>, Self)
    where
        M: Send + 'static,
    {
        let (req, reply) = Snd::new();
        let action = Action {
            phantom_data: PhantomData,
            handler_fn: fun.into_any(),
            params: Box::new((msg, req)),
        };
        (action, reply)
    }
}

// pub trait IntoAction<A>: Sized {
//     type Msg: Send;
//     type Out;
//     type Receiver;
//     fn into_action(self, msg: Self::Msg) -> Self::Out;
//     fn into_split_action(self, msg: Self::Msg) -> (Action<A>, Self::Receiver);
// }

// impl<A, M> IntoAction<A> for ActorFn<A, Snd<M>, ()>
// where
//     A: Actor,
//     M: Send + 'static,
// {
//     type Out = Action<A>;
//     type Msg = M;
//     type Receiver = ();

//     fn into_action(self, msg: M) -> Self::Out {
//         Action {
//             phantom_data: PhantomData,
//             handler_fn: self.into_any(),
//             params: Box::new((msg, ())),
//         }
//     }

//     fn into_split_action(self, msg: Self::Msg) -> (Action<A>, Self::Receiver) {
//         (self.into_action(msg), ())
//     }
// }

// impl<A, M, R> IntoAction<A> for ActorFn<A, Snd<M>, Rcv<R>>
// where
//     A: Actor,
//     M: Send + 'static,
//     R: Send + 'static,
// {
//     type Out = (Action<A>, Rcv<R>);
//     type Msg = M;
//     type Receiver = Rcv<R>;

//     fn into_action(self, msg: M) -> Self::Out {
//         let (req, reply) = Snd::new();
//         let action = Action {
//             phantom_data: PhantomData,
//             handler_fn: self.into_any(),
//             params: Box::new((msg, req)),
//         };
//         (action, reply)
//     }

//     fn into_split_action(self, msg: Self::Msg) -> (Action<A>, Self::Receiver) {
//         self.into_action(msg)
//     }
// }

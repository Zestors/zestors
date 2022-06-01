use crate::core::*;
use std::{any::Any, marker::PhantomData, pin::Pin};

/// A macro to more easily create an `Action`. See `Action::new` for docs.
///
/// ## Examples:
/// ```ignore
/// let action1 = Action!(MyActor::handle_fn, msg);
/// let action2 = Action::new(Fn!(MyActor::handle_fn), msg);
/// assert_eq!(action1, action2);
///
/// let (action1, _rcv) = Action!(MyActor::handle_fn_snd, msg);
/// let (action2, _rcv) = Action::new(Fn!(MyActor::handle_fn_snd), msg);
/// assert_eq!(action1, action2);
/// ```
#[macro_export]
macro_rules! Action {
    ($function:expr, $param:expr) => {
        $crate::core::action::Action::new($crate::Fn!($function), $param)
    };
}

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

// This is only for the PhantomData<A>. Everything else is Send.
unsafe impl<A: ?Sized> Send for Action<A> {}

impl<A> Action<A> {
    /// Handle this action.
    pub async fn handle(self, actor: &mut A, state: &mut State<A>) -> HandlerOutput<A>
    where
        A: Actor,
    {
        unsafe { self.handler_fn.call(actor, state, self.params).await }
    }

    /// Create a new action. If the `HandlerFn` sends back a value, this will return a tuple
    /// of `(Action<A>, Rcv<R>)`, otherwise this will just return `Action<A>`.
    pub fn new<M, R>(fun: HandlerFn<A, M, R>, msg: M) -> R::NewActionType<A>
    where
        M: Send + 'static,
        R: RcvPart,
    {
        R::new_action(fun, msg)
    }

    /// Same as `new`, except this will always return a tuple.
    pub fn new_split<M, R>(fun: HandlerFn<A, M, R>, msg: M) -> (Self, R)
    where
        M: Send + 'static,
        R: RcvPart,
    {
        R::new_split_action(fun, msg)
    }

    /// Attempt to downcast this back into the message kept inside.
    pub fn downcast<M, R>(self) -> Result<(M, R::SndPart), Self>
    where
        M: 'static,
        R: RcvPart + 'static,
    {
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

/// Anything that can be received by an actor as it's second argument.
/// 
/// Currently this is either `Snd<T>` or `()`, but more might be added in the future.
pub trait RcvPart: Sized + Send + 'static {
    /// The part that is returned when a new action is created.
    type SndPart;

    /// The full type returned when creating a new action for actor A.
    type NewActionType<A>;

    fn new_split_action<A, M>(fun: HandlerFn<A, M, Self>, msg: M) -> (Action<A>, Self)
    where
        M: Send + 'static;

    fn new_action<A, M>(fun: HandlerFn<A, M, Self>, msg: M) -> Self::NewActionType<A>
    where
        M: Send + 'static;
}

impl RcvPart for () {
    type SndPart = ();
    type NewActionType<A> = Action<A>;

    fn new_split_action<A, M>(fun: HandlerFn<A, M, Self>, msg: M) -> (Action<A>, ())
    where
        M: Send + 'static,
    {
        (Self::new_action(fun, msg), ())
    }

    fn new_action<A, M>(fun: HandlerFn<A, M, Self>, msg: M) -> Self::NewActionType<A>
    where
        M: Send + 'static,
    {
        Action {
            phantom_data: PhantomData,
            handler_fn: fun.into_any(),
            params: Box::new((msg, ())),
        }
    }
}

impl<R: Send + 'static> RcvPart for Rcv<R> {
    type SndPart = Snd<R>;
    type NewActionType<A> = (Action<A>, Rcv<R>);

    fn new_split_action<A, M>(fun: HandlerFn<A, M, Self>, msg: M) -> (Action<A>, Rcv<R>)
    where
        M: Send + 'static,
    {
        Self::new_action(fun, msg)
    }

    fn new_action<A, M>(fun: HandlerFn<A, M, Self>, msg: M) -> Self::NewActionType<A>
    where
        M: Send + 'static,
    {
        let (snd, rcv) = new_channel();
        let action = Action {
            phantom_data: PhantomData,
            handler_fn: fun.into_any(),
            params: Box::new((msg, snd)),
        };
        (action, rcv)
    }
}
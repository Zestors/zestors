use super::*;
use crate::core::*;
use std::mem::transmute;


//------------------------------------------------------------------------------------------------
//  Fn!
//------------------------------------------------------------------------------------------------

/// A macro for easily converting handler-`fn`s into `HandleFn`s.
///
/// The `HandleFn` can subsequently be used to create actions or call addresses.
///
/// ## Usage:
/// ```ignore
/// fn handle_message(&mut self, msg: u32, state: ActorState<Self>) -> FlowResult<Self> {
///     Ok(Flow::Cont)
/// }
///
/// let handle_fn = Fn!(handle_message); // is of type "HandlerFn<ActorName, u32, ()>"
/// let action = Action::new(handle_fn, 10); // is of type "Action<ActorName>"
/// ```
#[macro_export]
macro_rules! Fn {
    ($function:expr) => {
        unsafe {
            <$crate::core::HandlerFn<_, _, _> as $crate::core::FromFn<_, _>>::from_fn(
                $function as usize,
                $function,
            )
        }
    };
}

//------------------------------------------------------------------------------------------------
//  FromFn
//------------------------------------------------------------------------------------------------

/// A trait to help with the conversion of fn pointers to `HandleFn`s.
///
/// This is only necessary because `fn`-types cannot be coerced at all if there are multiple
/// possible implementations. That's why this function is called by a macro, that ensures the
/// ptr is the correct type. Then coercion can be dont using `Fn`-traits, and making sure the
/// `fn`s implement these traits.
pub trait FromFn<F, Ident>: Sized {
    unsafe fn from_fn(ptr: usize, function: F) -> Self;
}

/// Helper type for FromFn type-coercion
pub struct HandlerIdent1;
/// Helper type for FromFn type-coercion
pub struct HandlerIdent2;
/// Helper type for FromFn type-coercion
pub struct AsyncHandlerIdent1<Fut>(PhantomData<Fut>);
/// Helper type for FromFn type-coercion
pub struct AsyncHandlerIdent2<Fut>(PhantomData<Fut>);

//------------------------------------------------------------------------------------------------
//  FromFn implementation
//------------------------------------------------------------------------------------------------

impl<A, M, F> FromFn<F, HandlerIdent1> for HandlerFn<A, M, ()>
where
    A: Actor,
    M: Send + 'static,
    F: Fn(&mut A, M) -> FlowResult<A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: MsgHandlerFn1<A, M> = transmute(ptr);
        Self::new_msg_1(ptr)
    }
}

impl<'a, A, M, F> FromFn<F, HandlerIdent2> for HandlerFn<A, M, ()>
where
    A: Actor,
    M: Send + 'static,
    F: Fn(&'a mut A, M, &'a mut State<A>) -> FlowResult<A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: MsgHandlerFn2<A, M> = transmute(ptr);
        Self::new_msg_2(ptr)
    }
}

impl<'a, A, M, F, Fut> FromFn<F, AsyncHandlerIdent1<Fut>> for HandlerFn<A, M, ()>
where
    A: Actor,
    M: Send + 'static,
    F: Fn(&'a mut A, M) -> Fut,
    Fut: HandlerFut<'a, A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: AsyncMsgHandlerFn1<A, M, Fut> = transmute(ptr);
        Self::new_async_msg_1(ptr)
    }
}

impl<'a, A, M, F, Fut> FromFn<F, AsyncHandlerIdent2<Fut>> for HandlerFn<A, M, ()>
where
    A: Actor,
    M: Send + 'static,
    F: Fn(&'a mut A, M, &'a mut State<A>) -> Fut,
    Fut: HandlerFut<'a, A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: AsyncMsgHandlerFn2<A, M, Fut> = transmute(ptr);
        Self::new_async_msg_2(ptr)
    }
}

impl<'a, A, M, F, R> FromFn<F, HandlerIdent1> for HandlerFn<A, M, Rcv<R>>
where
    A: Actor,
    R: Send + 'static,
    M: Send + 'static,
    F: Fn(&'a mut A, M, Snd<R>) -> FlowResult<A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: ReqHandlerFn1<A, M, R> = transmute(ptr);
        Self::new_req_1(ptr)
    }
}

impl<'a, A, M, F, R> FromFn<F, HandlerIdent2> for HandlerFn<A, M, Rcv<R>>
where
    A: Actor,
    M: Send + 'static,
    R: Send + 'static,
    F: Fn(&'a mut A, M, Snd<R>, &'a mut State<A>) -> FlowResult<A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: ReqHandlerFn2<A, M, R> = transmute(ptr);
        Self::new_req_2(ptr)
    }
}

impl<'a, A, M, F, Fut, R> FromFn<F, AsyncHandlerIdent1<Fut>> for HandlerFn<A, M, Rcv<R>>
where
    A: Actor,
    R: Send + 'static,
    M: Send + 'static,
    F: Fn(&'a mut A, M, Snd<R>) -> Fut,
    Fut: HandlerFut<'a, A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: AsyncReqHandlerFn1<A, M, R, Fut> = transmute(ptr);
        Self::new_async_req_1(ptr)
    }
}

impl<'a, A, M, F, Fut, R> FromFn<F, AsyncHandlerIdent2<Fut>> for HandlerFn<A, M, Rcv<R>>
where
    A: Actor,
    R: Send + 'static,
    M: Send + 'static,
    F: Fn(&'a mut A, M, Snd<R>, &'a mut State<A>) -> Fut,
    Fut: HandlerFut<'a, A>,
{
    unsafe fn from_fn(ptr: usize, _function: F) -> Self {
        let ptr: AsyncReqHandlerFn2<A, M, R, Fut> = transmute(ptr);
        Self::new_async_req_2(ptr)
    }
}

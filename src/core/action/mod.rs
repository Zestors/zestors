use crate::core::*;
use futures::Future;
use std::marker::PhantomData;


//------------------------------------------------------------------------------------------------
//  mod
//------------------------------------------------------------------------------------------------

pub mod action;
pub mod handler_fn;
pub mod from_fn;

//------------------------------------------------------------------------------------------------
//  export
//------------------------------------------------------------------------------------------------

pub use from_fn::*;
pub use action::*;
pub use handler_fn::*;

//------------------------------------------------------------------------------------------------
//  helper-types
//------------------------------------------------------------------------------------------------

type MsgHandlerFn1<A, M> = fn(&mut A, M) -> FlowResult<A>;
type MsgHandlerFn2<A, M> = fn(&mut A, M, &mut State<A>) -> FlowResult<A>;
type AsyncMsgHandlerFn1<A, M, F> = fn(&mut A, M) -> F;
type AsyncMsgHandlerFn2<A, M, F> = fn(&mut A, M, &mut State<A>) -> F;

type ReqHandlerFn1<A, M, R> = fn(&mut A, M, Snd<R>) -> FlowResult<A>;
type ReqHandlerFn2<A, M, R> = fn(&mut A, M, Snd<R>, &mut State<A>) -> FlowResult<A>;
type AsyncReqHandlerFn1<A, M, R, F> = fn(&mut A, M, Snd<R>) -> F;
type AsyncReqHandlerFn2<A, M, R, F> = fn(&mut A, M, Snd<R>, &mut State<A>) -> F;



pub trait HandlerFut<'a, A: Actor>: Future<Output = FlowResult<A>> + Send + 'a {}
impl<'a, A: Actor, F> HandlerFut<'a, A> for F where F: Future<Output = FlowResult<A>> + Send + 'a {}
pub(crate) type HandlerOutput<A> = FlowResult<A>;

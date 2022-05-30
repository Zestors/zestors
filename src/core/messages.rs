use serde::{de::DeserializeOwned, Serialize};

use crate::distr::{Distr, RemoteMsg};

use super::{Action, Local, LocalAddrError, LocalMsg, Rcv};

//------------------------------------------------------------------------------------------------
//  Param
//------------------------------------------------------------------------------------------------

pub trait Param<'ze, T> {
    type T;
}

impl<'ze, T> Param<'ze, T> for Local {
    type T = T;
}

impl<'ze, T> Param<'ze, T> for Distr
where
    T: 'ze + Serialize + DeserializeOwned,
{
    type T = &'ze T;
}

//------------------------------------------------------------------------------------------------
//  Msg
//------------------------------------------------------------------------------------------------

pub trait Msg<T> {
    type T;
}

impl<T> Msg<T> for Local {
    type T = LocalMsg<T>;
}

impl<T> Msg<T> for Distr {
    type T = RemoteMsg<T>;
}

//------------------------------------------------------------------------------------------------
//  CallResult
//------------------------------------------------------------------------------------------------

pub trait CallResult<M, R> {
    type T;
}

impl<M, R> CallResult<M, Rcv<R>> for Local {
    type T = Result<(), LocalAddrError<R>>;
}

impl<M, R> CallResult<M, Rcv<R>> for Distr {
    type T = Result<(), LocalAddrError<R>>;
}

//------------------------------------------------------------------------------------------------
//  SendResult
//------------------------------------------------------------------------------------------------

pub trait SendResult<A> {
    type T;
}

impl<A> SendResult<A> for Local {
    type T = Result<(), LocalAddrError<Action<A>>>;
}

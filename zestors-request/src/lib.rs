#![doc = include_str!("../README.md")]

use std::marker::PhantomData;
use tokio::sync::oneshot;
use zestors_core::messaging::MessageType;

mod into_recv;
mod tx_rx;

pub use {into_recv::*, tx_rx::*};

//------------------------------------------------------------------------------------------------
//  Request
//------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct Request<T>(PhantomData<T>);

impl<T> Request<T> {
    pub fn create() -> (Tx<T>, Rx<T>) {
        let (tx, rx) = oneshot::channel();
        (Tx(tx), Rx(rx))
    }
}

impl<M, R> MessageType<M> for Request<R> {
    type Sends = (M, Tx<R>);
    type Returns = Rx<R>;

    fn new_pair(msg: M) -> ((M, Tx<R>), Rx<R>) {
        let (tx, rx) = Request::create();
        ((msg, tx), rx)
    }

    fn into_msg(sends: (M, Tx<R>), _returns: Rx<R>) -> M {
        sends.0
    }
}

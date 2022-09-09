#![doc = include_str!("../README.md")]

use std::marker::PhantomData;
use tokio::sync::oneshot;
use zestors_core::messaging::MessageType;

mod into_recv;
mod tx_rx;

pub use {into_recv::*, tx_rx::*};

pub fn new_request<T>() -> (Tx<T>, Rx<T>) {
    let (tx, rx) = oneshot::channel();
    (Tx(tx), Rx(rx))
}

impl<M, R> MessageType<M> for Rx<R> {
    type Sends = (M, Tx<R>);
    type Returns = Rx<R>;

    fn create(msg: M) -> ((M, Tx<R>), Rx<R>) {
        let (tx, rx) = new_request();
        ((msg, tx), rx)
    }

    fn destroy(sends: (M, Tx<R>), _returns: Rx<R>) -> M {
        sends.0
    }
}

impl<M, R> MessageType<M> for Tx<R> {
    type Sends = (M, Rx<R>);
    type Returns = Tx<R>;

    fn create(msg: M) -> ((M, Rx<R>), Tx<R>) {
        let (tx, rx) = new_request();
        ((msg, rx), tx)
    }

    fn destroy(sends: (M, Rx<R>), _returns: Tx<R>) -> M {
        sends.0
    }
}

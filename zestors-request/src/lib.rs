use tokio::sync::oneshot;
use zestors_core::messaging::MessageDerive;
mod into_recv;
mod tx_rx;
pub use {into_recv::*, tx_rx::*};

pub fn new<T>() -> (Tx<T>, Rx<T>) {
    let (tx, rx) = oneshot::channel();
    (Tx(tx), Rx(rx))
}

impl<M, R> MessageDerive<M> for Rx<R> {
    type Payload = (M, Tx<R>);
    type Returned = Rx<R>;

    fn create(msg: M) -> ((M, Tx<R>), Rx<R>) {
        let (tx, rx) = new();
        ((msg, tx), rx)
    }

    fn cancel(sent: (M, Tx<R>), _returned: Rx<R>) -> M {
        sent.0
    }
}

impl<M, R> MessageDerive<M> for Tx<R> {
    type Payload = (M, Rx<R>);
    type Returned = Tx<R>;

    fn create(msg: M) -> ((M, Rx<R>), Tx<R>) {
        let (tx, rx) = new();
        ((msg, rx), tx)
    }

    fn cancel(sent: (M, Rx<R>), _returned: Tx<R>) -> M {
        sent.0
    }
}

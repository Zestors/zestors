use futures::{Future, FutureExt, future::FusedFuture};

/// A oneshot channel used for sending an abort message.
pub(crate) struct AbortReceiver {
    receiver: oneshot::Receiver<()>,
}

/// A oneshot channel used for receiving an abort message
/// Dropping this will not abort the process
#[derive(Debug)]
pub(crate) struct AbortSender {
    sender: oneshot::Sender<()>,
}

impl AbortSender {
    /// Create a new abort channel
    pub(crate) fn new() -> (AbortSender, AbortReceiver) {
        let (sender, receiver) = oneshot::channel();
        (AbortSender { sender }, AbortReceiver { receiver })
    }

    /// Send the soft_abort
    pub(crate) fn send_soft_abort(self) {
        let _ = self.sender.send(());
    }
}

impl Future for AbortReceiver {
    type Output = AbortAction;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.receiver.poll_unpin(cx).map(|res| match res {
            // if a message was received, the process should abort
            Ok(()) => AbortAction::SoftAbort,
            // if no message was received, the process should not abort
            Err(_) => AbortAction::Isolated,
        })
    }
}

/// Whether this process should abort or not
pub(crate) enum AbortAction {
    /// The process should abort
    SoftAbort,
    /// The process has been detached
    Isolated,
}

use crate::{
    handler::{Handler, HandlerInterface},
    *,
};
use rootcause::Report;
use std::fmt::Debug;
use tokio::select;
use zestors_runtime::Signal;
use zestors_runtime::{ActorRef, InboxEvent, prelude::*};

pub(super) struct FullHandlerState<H: Handler> {
    inbox: Inbox<H::Interface>,
    address: Address<H::Interface>,
}

impl<H: Handler> FullHandlerState<H> {
    pub(super) fn new(inbox: Inbox<H::Interface>) -> Self {
        Self {
            address: inbox.address().clone(),
            inbox,
        }
    }

    pub(super) fn split(&mut self) -> (&mut Inbox<H::Interface>, HandlerState<'_, H>) {
        (
            &mut self.inbox,
            HandlerState {
                address: &self.address,
            },
        )
    }

    async fn exit(&mut self, handler: &mut H, reason: HandlerExit) -> Result<(), Report> {
        let (_, state) = self.split();
        handler.exit(state, reason).await
    }

    async fn init(&mut self, handler: &mut H) -> Result<(), InitError> {
        let (inbox, state) = self.split();

        tokio::select! {
            res = handler.init(state) => {
                res.map_err(InitError::Failed)
            }

            _shutdown_signal_received = async {
                while let Some(signal) = inbox.recv_signal().await {
                    match signal {
                        Signal::Shutdown => {
                            break;
                        }

                        Signal::Resume | Signal::Suspend => {
                            tracing::debug!("Ignoring signal {:?} while initializing", signal);
                        }
                    }
                }
            } => {
                Err(InitError::Cancelled)
            }
        }
    }

    pub(super) async fn run(&mut self, handler: &mut H) -> Result<(), Report>
    where
        H: Handler + Debug,
    {
        if let Err(e) = self.init(handler).await {
            return self.exit(handler, e.into()).await;
        }

        loop {
            match self.run_once(handler).await {
                Ok(RunOnce::Continue) => {}

                Ok(RunOnce::ExitNormal) => {
                    break self.exit(handler, HandlerExit::Normal).await;
                }

                Err(e) => {
                    break self.exit(handler, HandlerExit::HandlerError(e)).await;
                }
            }
        }
    }

    async fn run_once(&mut self, handler: &mut H) -> Result<RunOnce, Report> {
        let (inbox, state) = self.split();

        let msg = select! {
            msg = inbox.recv_event() => {
                if let Some(msg) = msg {
                    msg
                } else {
                    return Ok(RunOnce::ExitNormal);
                }
            }

            Some(result) = handler.next_event() => {
                result?.handle(state, handler).await?;
                return Ok(RunOnce::Continue);
            }
        };

        match msg {
            InboxEvent::Signal(signal) => match signal {
                Signal::Resume => {
                    handler.on_resume(&self.address).await?;
                    return Ok(RunOnce::Continue);
                }

                Signal::Suspend => {
                    handler.on_suspend(&self.address).await?;
                    return Ok(RunOnce::Continue);
                }

                Signal::Shutdown => {
                    handler.on_shutdown(&self.address).await?;
                    return Ok(RunOnce::Continue);
                }
            },

            InboxEvent::Message(msg) => {
                msg.handle_with(state, handler).await?;
                return Ok(RunOnce::Continue);
            }
        }
    }
}

impl<H: Handler> ActorRef for FullHandlerState<H> {
    type Ctx = H::Interface;

    fn actor_ref(&self) -> &Address<Self::Ctx> {
        &self.address
    }
}

enum InitError {
    Failed(Report),
    Cancelled,
}

impl From<InitError> for HandlerExit {
    fn from(e: InitError) -> Self {
        match e {
            InitError::Failed(e) => HandlerExit::InitError(e),
            InitError::Cancelled => HandlerExit::InitCancelled,
        }
    }
}

enum RunOnce {
    Continue,
    ExitNormal,
}

pub struct HandlerState<'a, H: Handler> {
    address: &'a Address<H::Interface>,
}

// impl<'a, H: Handler> HandlerState<'a, H> {
//     /// Schedule a future that will produce a [`Message`] to be handled by the actor.
//     pub fn schedule_msg<F, M>(&mut self, future_message: F)
//     where
//         F: Future<Output = Result<M, Report>> + Send + 'static,
//         M: Message,
//         H: Handle<M>,
//     {
//         self.scheduler.schedule_msg(future_message);
//     }

//     pub fn schedule_fut<F>(&mut self, future_message: F)
//     where
//         F: Future<Output = Result<(), Report>> + Send + 'static,
//     {
//         self.scheduler.schedule_fut(future_message);
//     }
// }

impl<'a, H: Handler> ActorRef for HandlerState<'a, H> {
    type Ctx = H::Interface;

    fn actor_ref(&self) -> &Address<Self::Ctx> {
        self.address
    }
}

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
                    Ok(RunOnce::Continue)
                }

                Signal::Suspend => {
                    handler.on_suspend(&self.address).await?;
                    Ok(RunOnce::Continue)
                }

                Signal::Shutdown => {
                    handler.on_shutdown(&self.address).await?;
                    Ok(RunOnce::Continue)
                }
            },

            InboxEvent::Message(msg) => {
                msg.handle_with(state, handler).await?;
                Ok(RunOnce::Continue)
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

/// The [`Handler`]'s view of its own actor, passed to lifecycle hooks and
/// message handlers. Gives access to the actor's own [`Address`] via
/// [`ActorRef`]/[`ActorOps`](zestors_runtime::ActorOps).
pub struct HandlerState<'a, H: Handler> {
    address: &'a Address<H::Interface>,
}

impl<'a, H: Handler> ActorRef for HandlerState<'a, H> {
    type Ctx = H::Interface;

    fn actor_ref(&self) -> &Address<Self::Ctx> {
        self.address
    }
}

use crate::all::*;
use async_trait::async_trait;
use futures::{future::BoxFuture, FutureExt};
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{ready, Context, Poll},
    time::Duration,
};

pub struct HandlerSpec<H, I> {
    init: I,
    handler: PhantomData<H>,
}

impl<H, I> HandlerSpec<H, I>
where
    H: HandleRestart<I>,
    I: Send + 'static,
{
    pub fn new(init: I) -> Self {
        Self {
            init,
            handler: PhantomData,
        }
    }
}

#[async_trait]
impl<H, I> Specification for HandlerSpec<H, I>
where
    H: HandleRestart<I>,
    I: Send + 'static,
{
    type Ref = H::Ref;
    type Supervisee = HandlerSupervisee<H, I>;

    async fn start_supervised(self) -> StartResult<Self> {
        match H::start(self.init).await {
            Ok((child, reference)) => Ok((
                HandlerSupervisee {
                    child,
                    init: PhantomData,
                    restart_fut: None,
                },
                reference,
            )),
            Err(e) => match H::handle_restart(RestartReason::InitError(e)).await {
                Ok(Some(init)) => Err(StartError::StartFailed(Self {
                    init,
                    handler: PhantomData,
                })),
                Ok(None) => Err(StartError::Completed),
                Err(e) => Err(StartError::Fatal(e)),
            },
        }
    }
}

pub struct HandlerSupervisee<H: Handler, I> {
    restart_fut: Option<BoxFuture<'static, Result<Option<I>, FatalError>>>,
    child: Child<H::Exit, HandlerInbox<H>>,
    init: PhantomData<I>,
}

impl<H: Handler, I> Unpin for HandlerSupervisee<H, I> {}

impl<H, I> Supervisee for HandlerSupervisee<H, I>
where
    H: HandleRestart<I>,
    I: Send + 'static,
{
    type Spec = HandlerSpec<H, I>;

    fn poll_supervise(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<SupervisionResult<Self::Spec>> {
        loop {
            if let Some(restart_fut) = &mut self.restart_fut {
                break restart_fut.poll_unpin(cx).map(|res| match res {
                    Ok(Some(init)) => Ok(Some(HandlerSpec {
                        init,
                        handler: PhantomData,
                    })),
                    Ok(None) => Ok(None),
                    Err(e) => Err(e),
                });
            } else {
                let exit = ready!(self.child.poll_unpin(cx));
                self.restart_fut = Some(H::handle_restart(RestartReason::Exit(exit)));
            }
        }
    }

    fn shutdown_time(self: Pin<&Self>) -> Duration {
        self.child.link().into_duration_or_default()
    }

    fn halt(self: Pin<&mut Self>) {
        self.child.halt()
    }

    fn abort(mut self: Pin<&mut Self>) {
        self.child.abort();
    }
}

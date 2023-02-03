use crate::all::*;
use futures::{Future, FutureExt, Stream, StreamExt};
use std::{
    any::Any,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use thiserror::Error;
use tokio::time::Sleep;

//------------------------------------------------------------------------------------------------
//  ShutdownFut
//------------------------------------------------------------------------------------------------

pub struct ShutdownFut<'a, E: Send + 'static, T: ActorType> {
    child: &'a mut Child<E, T>,
    sleep: Option<Pin<Box<Sleep>>>,
}

impl<'a, E: Send + 'static, T: ActorType> ShutdownFut<'a, E, T> {
    pub(crate) fn new(child: &'a mut Child<E, T>, timeout: Duration) -> Self {
        child.halt();

        ShutdownFut {
            child,
            sleep: Some(Box::pin(tokio::time::sleep(timeout))),
        }
    }
}

impl<'a, E: Send + 'static, T: ActorType> Unpin for ShutdownFut<'a, E, T> {}

impl<'a, E: Send + 'static, T: ActorType> Future for ShutdownFut<'a, E, T> {
    type Output = Result<E, ExitError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.child.poll_unpin(cx) {
            Poll::Ready(res) => Poll::Ready(res),
            Poll::Pending => {
                if let Some(sleep) = &mut self.sleep {
                    if sleep.poll_unpin(cx).is_ready() {
                        self.sleep = None;
                        self.child.abort();
                    }
                };
                Poll::Pending
            }
        }
    }
}

//------------------------------------------------------------------------------------------------
//  ShutdownStream
//------------------------------------------------------------------------------------------------

/// Stream returned when shutting down a [ChildGroup].
///
/// This stream can be collected into a vec with [StreamExt::collect]:
pub struct ShutdownStream<'a, E: Send + 'static, T: ActorType> {
    group: &'a mut ChildGroup<E, T>,
    sleep: Option<Pin<Box<Sleep>>>,
}

impl<'a, E: Send + 'static, T: ActorType> ShutdownStream<'a, E, T> {
    pub(super) fn new(group: &'a mut ChildGroup<E, T>, timeout: Duration) -> Self {
        group.halt();

        ShutdownStream {
            group,
            sleep: Some(Box::pin(tokio::time::sleep(timeout))),
        }
    }
}

impl<'a, E: Send + 'static, T: ActorType> Stream for ShutdownStream<'a, E, T> {
    type Item = Result<E, ExitError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(sleep) = &mut self.sleep {
            if sleep.poll_unpin(cx).is_ready() {
                self.sleep = None;
                self.group.abort();
            }
        };

        self.group.poll_next_unpin(cx)
    }
}

//------------------------------------------------------------------------------------------------
//  ExitError
//------------------------------------------------------------------------------------------------

/// An error returned from an exiting process.
#[derive(Debug, Error)]
pub enum ExitError {
    /// Process panicked.
    #[error("Process has exited because of a panic")]
    Panic(Box<dyn Any + Send>),
    /// Process  was aborted.
    #[error("Process has exited because it was aborted")]
    Abort,
}

impl ExitError {
    /// Whether the error is a panic.
    pub fn is_panic(&self) -> bool {
        match self {
            ExitError::Panic(_) => true,
            ExitError::Abort => false,
        }
    }

    /// Whether the error is an abort.
    pub fn is_abort(&self) -> bool {
        match self {
            ExitError::Panic(_) => false,
            ExitError::Abort => true,
        }
    }
}

impl From<tokio::task::JoinError> for ExitError {
    fn from(e: tokio::task::JoinError) -> Self {
        match e.try_into_panic() {
            Ok(panic) => ExitError::Panic(panic),
            Err(_) => ExitError::Abort,
        }
    }
}

//------------------------------------------------------------------------------------------------
//  Test`
//------------------------------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use crate::_priv::test_helper::{basic_actor, grouped_basic_actor};
    use crate::all::*;
    use futures::stream::StreamExt;
    use std::{future::pending, time::Duration};

    #[tokio::test]
    async fn shutdown_success() {
        let (mut child, _addr) = spawn(basic_actor!());
        assert!(child.shutdown(Duration::from_millis(5)).await.is_ok());
    }

    #[tokio::test]
    async fn shutdown_failure() {
        let (mut child, _addr) = spawn(|_inbox: Inbox<()>| async {
            pending::<()>().await;
        });
        assert!(matches!(
            child.shutdown(Duration::from_millis(5)).await,
            Err(ExitError::Abort)
        ));
    }

    #[tokio::test]
    async fn shutdown_group_success() {
        let (mut child, _addr) = spawn_group(0..3, grouped_basic_actor!());

        let results = child
            .shutdown(Duration::from_millis(5))
            .collect::<Vec<_>>()
            .await;
        assert_eq!(results.len(), 3);

        for result in results {
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn shutdown_group_failure() {
        let (mut child, _addr) = spawn_group(0..3, |_, _inbox: Inbox<()>| async {
            pending::<()>().await;
        });

        let results = child
            .shutdown(Duration::from_millis(5))
            .collect::<Vec<_>>()
            .await;
        assert_eq!(results.len(), 3);

        for result in results {
            assert!(matches!(result, Err(ExitError::Abort)));
        }
    }

    #[tokio::test]
    async fn shutdown_group_mixed() {
        let (child, _addr) = spawn(|_inbox: Inbox<()>| async move {
            pending::<()>().await;
            unreachable!()
        });
        let mut child = child.into_group();
        child.spawn_onto(basic_actor!()).unwrap();
        child
            .spawn_onto(|_inbox: Inbox<()>| async move {
                pending::<()>().await;
                unreachable!()
            })
            .unwrap();
        child.spawn_onto(basic_actor!()).unwrap();

        let results = child
            .shutdown(Duration::from_millis(5))
            .collect::<Vec<_>>()
            .await;

        let successes = results.iter().filter(|res| res.is_ok()).count();
        let failures = results.iter().filter(|res| res.is_err()).count();

        assert_eq!(successes, 2);
        assert_eq!(failures, 2);
    }
}

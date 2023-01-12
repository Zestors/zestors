use crate::*;
use futures::{Future, FutureExt, Stream, StreamExt};
use thiserror::Error;
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration, any::Any,
};
use tokio::time::Sleep;

//------------------------------------------------------------------------------------------------
//  ShutdownFut
//------------------------------------------------------------------------------------------------

pub struct ShutdownFut<'a, E: Send + 'static, T: DefinesChannel> {
    child: &'a mut Child<E, T>,
    sleep: Option<Pin<Box<Sleep>>>,
}

impl<'a, E: Send + 'static, T: DefinesChannel> ShutdownFut<'a, E, T> {
    pub(crate) fn new(child: &'a mut Child<E, T>, timeout: Duration) -> Self {
        child.halt();

        ShutdownFut {
            child,
            sleep: Some(Box::pin(tokio::time::sleep(timeout))),
        }
    }
}

impl<'a, E: Send + 'static, T: DefinesChannel> Unpin for ShutdownFut<'a, E, T> {}

impl<'a, E: Send + 'static, T: DefinesChannel> Future for ShutdownFut<'a, E, T> {
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

/// Stream returned when shutting down a [ChildPool].
///
/// This stream can be collected into a vec with [StreamExt::collect]:
pub struct ShutdownStream<'a, E: Send + 'static, T: DefinesChannel> {
    pool: &'a mut ChildPool<E, T>,
    sleep: Option<Pin<Box<Sleep>>>,
}

impl<'a, E: Send + 'static, T: DefinesChannel> ShutdownStream<'a, E, T> {
    pub(crate) fn new(pool: &'a mut ChildPool<E, T>, timeout: Duration) -> Self {
        pool.halt();

        ShutdownStream {
            pool,
            sleep: Some(Box::pin(tokio::time::sleep(timeout))),
        }
    }
}

impl<'a, E: Send + 'static, T: DefinesChannel> Stream for ShutdownStream<'a, E, T> {
    type Item = Result<E, ExitError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(sleep) = &mut self.sleep {
            if sleep.poll_unpin(cx).is_ready() {
                self.sleep = None;
                self.pool.abort();
            }
        };

        self.pool.poll_next_unpin(cx)
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
    use std::{future::pending, time::Duration};

    use futures::StreamExt;

    use crate::*;

    #[tokio::test]
    async fn shutdown_success() {
        let (mut child, _addr) = spawn(Config::default(), basic_actor!());
        assert!(child.shutdown(Duration::from_millis(5)).await.is_ok());
    }

    #[tokio::test]
    async fn shutdown_failure() {
        let (mut child, _addr) = spawn(Config::default(), |_inbox: Inbox<()>| async {
            pending::<()>().await;
        });
        assert!(matches!(
            child.shutdown(Duration::from_millis(5)).await,
            Err(ExitError::Abort)
        ));
    }

    #[tokio::test]
    async fn shutdown_pool_success() {
        let (mut child, _addr) =
        spawn_many(0..3, Config::default(), pooled_basic_actor!());

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
    async fn shutdown_pool_failure() {
        let (mut child, _addr) =
            spawn_many(0..3, Config::default(), |_, _inbox: Inbox<()>| async {
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
    async fn shutdown_pool_mixed() {
        let (mut child, _addr) =
            spawn_one(Config::default(), |_inbox: Inbox<()>| async move {
                pending::<()>().await;
                unreachable!()
            });
        child.spawn(basic_actor!()).unwrap();
        child
            .spawn(|_inbox: Inbox<()>| async move {
                pending::<()>().await;
                unreachable!()
            })
            .unwrap();
        child.spawn(basic_actor!()).unwrap();

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

use super::*;
use crate::core::*;
use async_trait::async_trait;
use futures::{future::BoxFuture, Future};
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

//------------------------------------------------------------------------------------------------
//  Spawnable
//------------------------------------------------------------------------------------------------

pub trait Spawnable {
    type Exit: Send + 'static;
    type ActorType: DefinesChannel + 'static;
    type Ref: Send + 'static;

    fn spawn(self) -> (Child<Self::Exit, Self::ActorType>, Self::Ref);
}

//------------------------------------------------------------------------------------------------
//  Startable
//------------------------------------------------------------------------------------------------

pub trait Startable {
    type Exit: Send + 'static;
    type ActorType: DefinesChannel + 'static;
    type Ref: Send + 'static;

    fn poll_start(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<(Child<Self::Exit, Self::ActorType>, Self::Ref), StartError>>;
}

//------------------------------------------------------------------------------------------------
//  Supervisable
//------------------------------------------------------------------------------------------------

pub trait Supervisable: Startable {
    /// Initialize to be able to call `poll_after_exit`.
    ///
    /// - `restart`: Whether the child is being restarted afterwards with `poll_restart`.
    /// - `exit`: The value that the child has exited with.
    fn init_poll_after_exit(
        self: Pin<&mut Self>,
        restart: bool,
        exit: Result<Self::Exit, ExitError>,
    );

    // fn run_after_exit(&mut self, )

    /// Method is called after `init_poll_after_exit`, when a child has exited.
    ///
    fn poll_after_exit(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<bool, StartError>>;

    fn poll_restart(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<(Child<Self::Exit, Self::ActorType>, Self::Ref), StartError>>;
}

pub trait SupervisableV2 {
    type Exit: Send + 'static;
    type ActorType: DefinesChannel + 'static;
    type Ref: Send + 'static;

    fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context) -> Poll<()>;

    fn child(self: Pin<&mut Self>) -> &mut Child<Self::Exit, Self::ActorType>;
}

trait Test {
    type Future<'a>: Future + 'a + Send
    where
        Self: 'a;

    fn test(&mut self) -> Self::Future<'_>;
}

impl Test for String {
    type Future<'a> = BoxFuture<'a, Option<&'a String>>;

    fn test(&mut self) -> Self::Future<'_> {
        Box::pin(async move {
            if self == "true" {
                Some(&*self)
            } else {
                None
            }
        })
    }
}

struct MapTest<T, R>(T, for<'a> fn(<T::Future<'a> as Future>::Output) -> R)
where
    T: Test + Send + 'static,
    for<'a> T::Future<'a>: Future + Send;

impl<T, R> Test for MapTest<T, R>
where
    T: Test + Send + 'static,
    for<'a> T::Future<'a>: Future + Send,
{
    type Future<'a> = BoxFuture<'a, R>
    where
        Self: 'a;

    fn test(&mut self) -> Self::Future<'_> {
        Box::pin(async move {
            let x = self.0.test().await;
            (self.1)(x)
        })
    }
}

async fn test() {
    let mut string = String::from("test");

    let fut1 = string.test();
    let _res = fut1.await;

    let mut map_test = MapTest(string, |x| true);
    let fut2 = map_test.test();
    let _res = fut2.await;
}

#[async_trait]
pub trait TerminalSupervisable {
    /// Run the supervisor
    fn poll_supervise(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), FatalError>>;

    /// The supervisor has been halted, and all children must now exit
    fn poll_halt(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), FatalError>>;
}

pub struct FatalError;

pub enum SupervisionError {
    Fatal,
}

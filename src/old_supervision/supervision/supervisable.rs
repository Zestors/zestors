use crate::{self as zestors, *};
use async_trait::async_trait;
use futures::Future;
use std::{mem::swap, pin::Pin, time::Duration};

pub enum RestartError {
    CouldNotStart,
}

/// The trait that has to be implemented for your child in order to register it under
/// a supervisor.
#[async_trait]
pub trait Supervisable {
    /// Supervise the process until it exits.
    ///
    /// This returns `false` if the process has exited successfully, and should not be restarted.
    /// If this returns `true`, the supervisor will attempt to restart the process.
    async fn supervise(&mut self) -> bool;

    /// If the supervisor attempts to restart the child, this function is called to restart the process.
    ///
    /// In theory this method should never fail. If an error is returned here, the supervisor will also
    /// exit and its supervisor will try to restart it.
    async fn restart(&mut self) -> Result<(), RestartError>;

    fn abort(&mut self) -> bool;
    fn halt(&self);
    fn halt_some(&self, count: u32);
}

//------------------------------------------------------------------------------------------------
//  State
//------------------------------------------------------------------------------------------------

pub(crate) enum ChildState<F2> {
    /// Child is alive
    Alive,
    /// Child has exited
    Exited,
    /// The child is in the process of being restarted
    Restarting(Pin<Box<F2>>),
    /// Child has exited and won't restart
    Finished,
}

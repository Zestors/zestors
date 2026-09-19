//! Sending to a remote actor: the [`RemoteAccepts`] trait and its options.

use super::{
    RemoteActorRef, RemoteCallError, RemoteCastError, RemoteMessage, RemoteReceipt,
    actor_ops::Route, cluster_address,
};
use std::{future::Future, time::Duration};
use type_sets::Contains;
use zestors_runtime::Context;

/// Options for a single message sent with [`RemoteAccepts`], the remote
/// counterpart of [`CallOptions`](zestors_runtime::CallOptions).
///
/// ```
/// # use zestors_distr::RemoteCallOptions;
/// # use std::time::Duration;
/// let options = RemoteCallOptions::new().timeout(Duration::from_secs(2));
/// assert_eq!(options.timeout, Some(Duration::from_secs(2)));
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct RemoteCallOptions {
    /// How long to wait for the reply, instead of the address's or the node's
    /// default.
    pub timeout: Option<Duration>,
}

impl RemoteCallOptions {
    /// Creates a new [`RemoteCallOptions`] with nothing set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets [`RemoteCallOptions::timeout`].
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

/// Message-sending operations for a reference to a remote actor: the remote
/// counterpart of [`Accepts`](zestors_runtime::Accepts), and it works the same
/// way.
///
/// It is implemented for a [`RemoteAddress<C>`](super::RemoteAddress) for every [`RemoteMessage`] that
/// `C` accepts, so only messages the actor is expected to take can be sent.
/// Sending returns the message's [`RemoteMessage::RemoteReceipt`]: `()` for a
/// message that expects no reply, and a [`RemoteReply`](super::RemoteReply) to wait for the reply
/// of one that does. [`call`](Self::call) sends and waits for it.
///
/// There are two ways to send:
///
/// - [`cast`](Self::cast) waits for room to send, and so only fails if the
///   message can't be sent at all.
/// - [`try_cast`](Self::try_cast) never waits: it also fails with
///   [`RemoteCastError::Full`] if many messages are queued for the node.
///
/// Returning means that the message is queued for sending, not that it arrived
/// or was accepted; for that, wait for the reply. Unlike a local message, one
/// can fail on the way: see [`RemoteReplyError`](super::RemoteReplyError). A message that got no answer
/// is not sent again; delivery is at most once.
pub trait RemoteAccepts<M: RemoteMessage>: Sync {
    /// Sends a message, waiting for room to send it if many messages are
    /// queued for the node.
    ///
    /// Equivalent to [`RemoteAccepts::cast_with`] with the default
    /// [`RemoteCallOptions`].
    fn cast(
        &self,
        msg: M,
    ) -> impl Future<Output = Result<M::RemoteReceipt, RemoteCastError<M>>> + Send {
        self.cast_with(msg, Default::default())
    }

    /// Same as [`RemoteAccepts::cast`], with explicit [`RemoteCallOptions`].
    fn cast_with(
        &self,
        msg: M,
        options: RemoteCallOptions,
    ) -> impl Future<Output = Result<M::RemoteReceipt, RemoteCastError<M>>> + Send;

    /// Sends a message immediately, without waiting for room.
    ///
    /// Equivalent to [`RemoteAccepts::try_cast_with`] with the default
    /// [`RemoteCallOptions`].
    fn try_cast(&self, msg: M) -> Result<M::RemoteReceipt, RemoteCastError<M>> {
        self.try_cast_with(msg, Default::default())
    }

    /// Same as [`RemoteAccepts::try_cast`], with explicit [`RemoteCallOptions`].
    ///
    /// Fails with [`RemoteCastError::Full`] if many messages are queued for the
    /// node.
    fn try_cast_with(
        &self,
        msg: M,
        options: RemoteCallOptions,
    ) -> Result<M::RemoteReceipt, RemoteCastError<M>>;

    /// Sends a message via [`RemoteAccepts::cast`] and waits for its reply.
    ///
    /// Equivalent to calling [`RemoteAccepts::cast`] and then
    /// [`RemoteReceipt::wait`] on the result, so it shares `cast`'s failures and
    /// adds those of getting the reply. The output is [`Message::Output`](zestors_interface::Message::Output), the
    /// reply. For a message that expects no reply, that is `()` as soon as the
    /// message is queued.
    fn call(&self, msg: M) -> impl Future<Output = Result<M::Output, RemoteCallError<M>>> + Send {
        self.call_with(msg, Default::default())
    }

    /// Same as [`RemoteAccepts::call`], with explicit [`RemoteCallOptions`].
    fn call_with(
        &self,
        msg: M,
        options: RemoteCallOptions,
    ) -> impl Future<Output = Result<M::Output, RemoteCallError<M>>> + Send {
        async move {
            let receipt = self.cast_with(msg, options).await?;
            receipt.wait().await.map_err(RemoteCallError::Reply)
        }
    }
}

/// One implementation for every kind of address: [`RemoteAddress`](super::RemoteAddress)
/// and [`ClusterAddress`](super::ClusterAddress) both are sent to through it.
impl<M, T> RemoteAccepts<M> for T
where
    M: RemoteMessage,
    T: RemoteActorRef,
    <T::Ctx as Context>::Set: Contains<M>,
{
    async fn cast_with(
        &self,
        msg: M,
        options: RemoteCallOptions,
    ) -> Result<M::RemoteReceipt, RemoteCastError<M>> {
        match self.route() {
            Route::Remote(address) => address.cast_remote(msg, options).await,
            Route::Local(address) => cluster_address::cast(address, msg, options).await,
        }
    }

    fn try_cast_with(
        &self,
        msg: M,
        options: RemoteCallOptions,
    ) -> Result<M::RemoteReceipt, RemoteCastError<M>> {
        match self.route() {
            Route::Remote(address) => address.try_cast_remote(msg, options),
            Route::Local(address) => cluster_address::try_cast(address, msg, options),
        }
    }
}

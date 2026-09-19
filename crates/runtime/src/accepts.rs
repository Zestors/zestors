use crate::*;

/// Provides message-sending operations for a channel.
///
/// `Accepts` exposes two ways to send a message, both configurable through a
/// shared [`CallOptions`]:
///
/// - [`Accepts::cast`] / [`Accepts::cast_with`] wait out backpressure before
///   sending, and so only fail if the channel is closed.
/// - [`Accepts::try_cast`] / [`Accepts::try_cast_with`] never wait: they fail
///   immediately if the channel is closed, and may also fail if it is
///   currently under backpressure (see [`Accepts::try_cast_with`]).
///
/// [`Accepts::call`] / [`Accepts::call_with`] build on `cast`/`cast_with` to
/// additionally wait for the message's reply.
///
/// # Example
///
/// ```
/// # use zestors::interface::{Envelope, Interface, Message};
/// # use zestors::runtime::prelude::*;
/// # use zestors::runtime::spawn_rand;
///
/// #[derive(Message, Debug)]
/// #[msg(reply = u32)]
/// # #[zestors(interface_path = "zestors::interface")]
/// struct Double(u32);
///
/// #[derive(Interface, Debug)]
/// # #[zestors(interface_path = "zestors::interface")]
/// enum MyInterface {
///     Double(Envelope<Double>),
/// }
///
/// # #[tokio::main]
/// # async fn main() {
/// let child = spawn_rand(|mut inbox: Inbox<MyInterface>| async move {
///     while let Some(MyInterface::Double(envelope)) = inbox.recv().await {
///         let n = envelope.msg.0;
///         let _ = envelope.reply(n * 2);
///     }
///     Ok(())
/// });
///
/// // `call` sends the message and waits for its reply.
/// let reply = child.call(Double(21)).await.unwrap();
/// assert_eq!(reply, 42);
///
/// child.signal_shutdown();
/// # }
/// ```
pub trait Accepts<M: Message>: Sync {
    /// Sends a message, waiting out backpressure first if the channel is
    /// under load.
    ///
    /// Equivalent to [`Accepts::cast_with`] with the default [`CallOptions`].
    fn cast(&self, msg: M) -> impl Future<Output = Result<ReceiptOf<M>, CastError<M>>> + Send {
        self.cast_with(msg, Default::default())
    }

    /// Same as [`Accepts::cast`], with explicit [`CallOptions`].
    ///
    /// Unless `options.ignore_backpressure` is `true`, this first waits for as
    /// long as the channel's current backpressure delay dictates (based on how
    /// full the channel is), then sends regardless of backpressure. Because of
    /// this, `cast_with` never fails due to the channel being full — its only
    /// failure mode is [`CastError`], returned if the channel is closed (which,
    /// unless `options.ignore_exiting` is `true`, includes a channel that is
    /// [`ActorStatus::Exiting`]).
    fn cast_with(
        &self,
        msg: M,
        options: CallOptions,
    ) -> impl Future<Output = Result<ReceiptOf<M>, CastError<M>>> + Send;

    /// Sends a message immediately, without waiting out backpressure.
    ///
    /// Equivalent to [`Accepts::try_cast_with`] with the default [`CallOptions`].
    fn try_cast(&self, msg: M) -> Result<ReceiptOf<M>, TryCastError<M>> {
        self.try_cast_with(msg, Default::default())
    }

    /// Same as [`Accepts::try_cast`], with explicit [`CallOptions`].
    ///
    /// Returns [`TryCastError::Closed`] if the channel is closed (which,
    /// unless `options.ignore_exiting` is `true`, includes a channel that is
    /// [`ActorStatus::Exiting`]).
    ///
    /// Returns [`TryCastError::Full`] if `options.ignore_backpressure` is
    /// `false` and the channel is currently under backpressure.
    fn try_cast_with(&self, msg: M, options: CallOptions) -> Result<ReceiptOf<M>, TryCastError<M>>;

    /// Sends a message via [`Accepts::cast`] and waits for its reply.
    ///
    /// Equivalent to calling [`Accepts::cast`] and then [`Receipt::wait`] on the
    /// result, so it shares `cast`'s backpressure and closed-channel behavior.
    /// The output is therefore [`Message::Output`] (the reply) rather than
    /// [`Message::Receipt`] (the handle used to await it).
    ///
    /// Returns [`CallError::Closed`] if the channel was closed at the time of
    /// sending, or [`CallError::NoResponse`] if no reply was ever received
    /// (for example, because the actor exited, or dropped the request, before
    /// replying).
    fn call(&self, msg: M) -> impl Future<Output = Result<M::Output, CallError<M>>> + Send {
        async move { Ok(self.cast(msg).await?.wait().await?) }
    }

    /// Same as [`Accepts::call`], with explicit [`CallOptions`] applied to the
    /// underlying [`Accepts::cast_with`] call.
    fn call_with(
        &self,
        msg: M,
        options: CallOptions,
    ) -> impl Future<Output = Result<M::Output, CallError<M>>> + Send {
        async move { Ok(self.cast_with(msg, options).await?.wait().await?) }
    }
}

impl<M, T> Accepts<M> for T
where
    T: ActorRef + Sync,
    M: Message,
    Address<T::Ctx>: Casts<M>,
{
    async fn cast_with(&self, msg: M, options: CallOptions) -> Result<ReceiptOf<M>, CastError<M>> {
        self.actor_ref()._cast_with(msg, options).await
    }

    fn try_cast_with(&self, msg: M, options: CallOptions) -> Result<ReceiptOf<M>, TryCastError<M>> {
        self.actor_ref()._try_cast_with(msg, options)
    }
}

/// Options controlling how a [`Accepts`]/[`ActorOps`] sending method behaves.
/// The default, used by [`Accepts::cast`]/[`Accepts::try_cast`]/etc., disables both
/// options below.
///
/// Built with a small setter per field:
///
/// ```
/// # use zestors::runtime::CallOptions;
/// let options = CallOptions::new().ignore_exiting(true);
/// assert!(options.ignore_exiting);
/// assert!(!options.ignore_backpressure);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct CallOptions {
    /// If `true`, a message is still accepted while the channel is
    /// [`ActorStatus::Exiting`]. Has no effect once the channel is fully
    /// [`ActorStatus::Exited`], which always rejects new messages.
    pub ignore_exiting: bool,
    /// If `true`, backpressure is ignored entirely: waiting methods (e.g.
    /// [`Accepts::cast`]) skip their delay, and non-waiting methods (e.g.
    /// [`Accepts::try_cast`]) skip the check that would otherwise return a
    /// full-channel error.
    pub ignore_backpressure: bool,
}

impl Default for CallOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl CallOptions {
    /// Creates a new [`CallOptions`] with both options disabled.
    pub fn new() -> Self {
        Self {
            ignore_exiting: false,
            ignore_backpressure: false,
        }
    }

    /// Sets [`CallOptions::ignore_exiting`].
    pub fn ignore_exiting(mut self, ignore: bool) -> Self {
        self.ignore_exiting = ignore;
        self
    }

    /// Sets [`CallOptions::ignore_backpressure`].
    pub fn ignore_backpressure(mut self, ignore: bool) -> Self {
        self.ignore_backpressure = ignore;
        self
    }
}

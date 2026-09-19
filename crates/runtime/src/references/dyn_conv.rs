use super::*;
use type_sets::{AsTypeSet, Members, Subset};

/// Converts a reference type (e.g. [`Address`], [`StrongAddress`], [`Child`])
/// from one [`Context`] into another, by value.
///
/// This is implemented for every actor reference type; [`Self::Ref`] is the
/// same reference type, but parameterized over the new [`Context`].
///
/// ```
/// # use zestors::interface::{Envelope, Interface, Message};
/// # use zestors::runtime::prelude::*;
/// # use zestors::runtime::{Dyn, spawn_rand};
/// #[derive(Message, Debug)]
/// # #[zestors(interface_path = "zestors::interface")]
/// struct Ping;
///
/// #[derive(Interface, Debug)]
/// # #[zestors(interface_path = "zestors::interface")]
/// enum PingInterface {
///     Ping(Envelope<Ping>),
/// }
///
/// # #[tokio::main]
/// # async fn main() {
/// let child = spawn_rand(|mut inbox: Inbox<PingInterface>| async move {
///     while inbox.recv().await.is_some() {}
///     Ok(())
/// });
///
/// // Widen to a `Dyn` address that only knows about `Ping`, e.g. to hand to
/// // code that shouldn't need to know the actor's full `Interface`...
/// let dyn_address: Address<Dyn<(Ping,)>> = child.address().clone().into_dyn();
///
/// // ...and narrow back to the concrete interface later.
/// let concrete = dyn_address.downcast::<PingInterface>().unwrap();
/// concrete.cast(Ping).await.unwrap();
///
/// child.signal_shutdown();
/// # }
/// ```
pub trait IntoDyn: ActorRef + Sized {
    /// `Self`, but reparameterized over the [`Context`] `T`.
    type Ref<T: Context>;

    /// Converts to the [`Context`] `C` without checking that the underlying
    /// channel actually supports it.
    ///
    /// # Safety-adjacent note
    /// This does not perform an unsafe cast, but an incorrect `C` will cause
    /// later operations (e.g. sending a message not accepted by `C`) to panic.
    /// Prefer [`IntoDyn::into_dyn`], [`IntoDyn::into_dyn_checked`], or
    /// [`IntoDyn::downcast`].
    fn into_context_unchecked<C>(self) -> Self::Ref<C>
    where
        C: Context;

    /// Widens the reference to a [`Dyn`] [`Context`] over the message set `S`.
    ///
    /// This is statically checked at compile time: `S` must be a subset of the
    /// current [`Context`]'s message set.
    fn into_dyn<S>(self) -> Self::Ref<Dyn<S>>
    where
        S: Subset<<Self::Ctx as Context>::Set> + AsTypeSet + 'static,
    {
        self.into_context_unchecked()
    }

    /// Attempts to widen the reference to a [`Dyn`] [`Context`] over the
    /// message set `S`, checked at runtime. Returns `self` unchanged in
    /// `Err` if the channel doesn't accept every message in `S`.
    fn into_dyn_checked<S>(self) -> Result<Self::Ref<Dyn<S>>, Self>
    where
        S: AsTypeSet + 'static + Members,
    {
        if self.is_superset_of(S::members()) {
            Ok(self.into_context_unchecked())
        } else {
            Err(self)
        }
    }

    /// Attempts to downcast the reference to the concrete [`Interface`] `I`,
    /// checked at runtime. Returns `self` unchanged in `Err` if the channel's
    /// interface is not `I`.
    fn downcast<I>(self) -> Result<Self::Ref<I>, Self>
    where
        I: Interface,
    {
        if self.is_interface::<I>() {
            Ok(self.into_context_unchecked())
        } else {
            Err(self)
        }
    }
}

/// Like [`IntoDyn`], but converts by reference instead of by value.
pub trait AsDyn: IntoDyn {
    /// Converts to the [`Context`] `S` without checking that the underlying
    /// channel actually supports it. See [`IntoDyn::into_context_unchecked`].
    fn as_context_unchecked<S>(&self) -> &Self::Ref<S>
    where
        S: Context;

    /// Widens the reference to a [`Dyn`] [`Context`] over the message set `S`.
    /// See [`IntoDyn::into_dyn`].
    fn as_dyn<S>(&self) -> &Self::Ref<Dyn<S>>
    where
        S: Subset<<Self::Ctx as Context>::Set> + AsTypeSet + 'static,
    {
        self.as_context_unchecked()
    }

    /// Attempts to widen the reference to a [`Dyn`] [`Context`] over the
    /// message set `S`, checked at runtime. See [`IntoDyn::into_dyn_checked`].
    fn as_dyn_checked<S>(&self) -> Option<&Self::Ref<Dyn<S>>>
    where
        S: AsTypeSet + 'static + Members,
    {
        if self.is_superset_of(S::members()) {
            Some(self.as_context_unchecked())
        } else {
            None
        }
    }

    /// Attempts to downcast the reference to the concrete [`Interface`] `I`,
    /// checked at runtime. See [`IntoDyn::downcast`].
    fn downcast_ref<I>(&self) -> Option<&Self::Ref<I>>
    where
        I: Interface,
    {
        if self.is_interface::<I>() {
            Some(self.as_context_unchecked())
        } else {
            None
        }
    }
}

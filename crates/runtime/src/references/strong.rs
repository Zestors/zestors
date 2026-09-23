use super::*;
use crate::registry::Registry;
use jiff::Zoned;
use std::{fmt::Debug, hash::Hash};

/// A strong version of [`Address`], which allows the actor to spawn
/// a new task after the previous one has exited. Once all strong references to a
/// channel are dropped, the channel is permanently closed, and the address is
/// removed from the [`Registry`].
///
/// [`Child`] and [`Inbox`] both contain a [`StrongAddress`]. Addresses can
/// be upgraded to a `StrongAddress`.
#[repr(transparent)]
pub struct StrongAddress<C: Context = Dyn> {
    address: Address<C>,
}

impl<T: Context> StrongAddress<T> {
    /// Creates a new channel with the given `name` and registers it in the
    /// local registry.
    pub fn create(name: Name) -> Result<Self, DuplicateNameError>
    where
        T: Interface,
    {
        // Register the plain `Address` first, and only wrap it into a
        // `StrongAddress` once that succeeds. `Address` has no `Drop` of its
        // own, but `StrongAddress` does: it removes its name's entry from the
        // registry once its strong count reaches zero. Wrapping eagerly
        // (before knowing whether registration succeeded) meant a *rejected*
        // duplicate-name attempt would still construct and then drop a full
        // `StrongAddress`, deregistering the entry actually owned by the
        // pre-existing address under that name.
        let address = Address::new(name, 1);

        Registry::local()
            .register(address.clone())
            .map_err(|_e| DuplicateNameError {
                name: address.name().clone(),
            })?;

        Ok(StrongAddress { address })
    }

    pub(crate) fn from_address_ref(handle: &Address<T>) -> Option<Self> {
        if handle.is_permanently_dead() {
            return None;
        }
        handle._channel().incr_strong_count();
        Some(Self {
            address: handle._clone(),
        })
    }
}

impl<C: Context> Drop for StrongAddress<C> {
    fn drop(&mut self) {
        if self.channel().decr_strong_count() {
            let removed_address = Registry::local().remove(self.name());

            if removed_address.is_none() {
                if cfg!(debug_assertions) {
                    panic!(
                        "Address {} was not found in the registry when dropping the last strong reference",
                        self.name()
                    );
                } else {
                    tracing::error!(
                        "Address {} was not found in the registry when dropping the last strong reference",
                        self.name()
                    );
                }
            }
        }
    }
}

impl<T: Context> Clone for StrongAddress<T> {
    fn clone(&self) -> Self {
        self.channel().incr_strong_count();

        StrongAddress {
            address: self.address._clone(),
        }
    }
}

impl<C: Context> IntoDyn for StrongAddress<C> {
    type Ref<T: Context> = StrongAddress<T>;

    fn into_context_unchecked<S>(self) -> Self::Ref<S>
    where
        S: Context,
    {
        unsafe { std::mem::transmute::<StrongAddress<C>, StrongAddress<S>>(self) }
    }
}

impl<C: Context> AsDyn for StrongAddress<C> {
    fn as_context_unchecked<S>(&self) -> &StrongAddress<S>
    where
        S: Context,
    {
        unsafe { std::mem::transmute::<&StrongAddress<C>, &StrongAddress<S>>(self) }
    }
}

impl<C: Context> ActorRef for StrongAddress<C> {
    type Ctx = C;

    fn actor_ref(&self) -> &Address<Self::Ctx> {
        &self.address
    }
}

impl<T: Context> Debug for StrongAddress<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Address<T> as Debug>::fmt(&self.address, f)
    }
}

impl<T: Context> PartialEq for StrongAddress<T> {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name()
    }
}
impl<T: Context> Eq for StrongAddress<T> {}

impl<T: Context> Hash for StrongAddress<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name().hash(state);
    }
}

/// A snapshot of a channel's status, queue lengths and history, returned by
/// [`ActorOps::snapshot`]. The fields are read one after another without
/// stopping the actor, so under load they can be a moment apart.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelSnapshot {
    /// The actor's [`Name`].
    pub name: Name,
    /// The actor's [`ActorStatus`] at the time of the snapshot.
    pub status: ActorStatus,
    /// The number of signals queued at the time of the snapshot.
    pub signal_len: usize,
    /// The number of messages queued at the time of the snapshot.
    pub msg_len: usize,
    /// Timestamps of the most recent spawns on this channel, oldest first
    /// (a bounded history - see [`ActorOps::spawned_at`]).
    pub spawns: Vec<Zoned>,
    /// Timestamps and outcomes of the most recent exits on this channel,
    /// oldest first (also a bounded history).
    pub exits: Vec<(Zoned, ExitStatus)>,
    /// When the channel itself was created; see [`ActorOps::created_at`].
    pub created_at: Zoned,
}

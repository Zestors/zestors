use super::*;
use crate::registry::Registry;
use jiff::Zoned;
use std::{fmt::Debug, hash::Hash};
use type_sets::AsTypeSet;

/// A strong version of [`Address`], which allows the [`Channel`] to spawn
/// a new task after the previous one has exited. Once all strong references to a
/// channel are dropped, the channel is permanently closed, and the address is
/// removed from the [`Registry`].
///
/// [`Child`] and [`Inbox`] both contain a [`StrongAddress`]. Addresses can
/// be upgraded to a `StrongAddress`.
#[repr(transparent)]
pub struct StrongAddress<C: Context = Set<()>> {
    channel: Channel<C>,
}

impl<T: Context> StrongAddress<T> {
    /// Creates a new channel with the given `pid` and registers it in the
    /// local registry.
    pub fn create(pid: Pid) -> Result<Self, DuplicatePidError>
    where
        T: Interface,
    {
        let this = StrongAddress {
            channel: Channel::new(pid, 1),
        };

        Registry::local()
            .register(this.address().clone())
            .map_err(|_e| DuplicatePidError {
                pid: this.pid().clone(),
            })?;

        Ok(this)
    }

    pub(crate) fn from_channel_ref(handle: &Channel<T>) -> Option<Self> {
        if handle.is_permanently_dead() {
            return None;
        }
        handle.incr_strong_count();
        Some(Self {
            channel: handle._clone(),
        })
    }
}

impl<C: Context> Drop for StrongAddress<C> {
    fn drop(&mut self) {
        self.channel.decr_strong_count();
    }
}

impl<T: Context> Clone for StrongAddress<T> {
    fn clone(&self) -> Self {
        self.handle().incr_strong_count();

        StrongAddress {
            channel: self.channel._clone(),
        }
    }
}

impl<C: Context> IntoDyn for StrongAddress<C> {
    type Ref<T: Context> = StrongAddress<T>;

    fn into_dyn_unchecked<S>(self) -> StrongAddress<S>
    where
        S: Context,
    {
        unsafe { std::mem::transmute::<StrongAddress<C>, StrongAddress<S>>(self) }
    }
}

impl<C: Context> AsDyn for StrongAddress<C> {
    fn as_dyn_unchecked<S>(&self) -> &StrongAddress<S>
    where
        S: Context,
    {
        unsafe { std::mem::transmute::<&StrongAddress<C>, &StrongAddress<S>>(self) }
    }
}

impl<C: Context> ActorOps for StrongAddress<C> {
    type Ctx = C;

    fn handle(&self) -> &Channel<Self::Ctx> {
        &self.channel
    }
}

impl<T: Context> Debug for StrongAddress<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Channel<T> as Debug>::fmt(&self.channel, f)
    }
}

impl<T: Context> PartialEq for StrongAddress<T> {
    fn eq(&self, other: &Self) -> bool {
        self.pid() == other.pid()
    }
}
impl<T: Context> Eq for StrongAddress<T> {}

impl<T: Context> Hash for StrongAddress<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.pid().hash(state);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChannelSnapshot {
    pub pid: Pid,
    pub status: ActorStatus,
    pub signal_len: usize,
    pub msg_len: usize,
    pub spawns: Vec<Zoned>,
    pub exits: Vec<(Zoned, ExitStatus)>,
    pub created_at: Zoned,
}

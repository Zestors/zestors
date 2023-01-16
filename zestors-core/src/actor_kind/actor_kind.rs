use crate::*;
use std::{any::TypeId, sync::Arc};

/// A channel-definition is part of any [Address] or [Child], and defines the [Channel] used in the
/// actor and which messages the actor [Accept]. The different channel-definitions are:
/// - A [`Protocol`]. (sized)
/// - A [`Halter`]. (sized)
/// - A [`Dyn<_>`] type using the [`Accepts!`] macro. (dynamic)
pub trait ActorKind {
    type Channel: DynChannel + ?Sized;
    fn into_dyn_channel(channel: Arc<Self::Channel>) -> Arc<dyn DynChannel>;
}

impl<D: ?Sized> ActorKind for Dyn<D> {
    type Channel = dyn DynChannel;

    fn into_dyn_channel(channel: Arc<Self::Channel>) -> Arc<dyn DynChannel> {
        channel
    }
}

/// A specialization of [`ActorKind`] for dynamic channels only.
pub trait DynActorKind: ActorKind<Channel = dyn DynChannel> {
    fn msg_ids() -> Box<[TypeId]>;
}

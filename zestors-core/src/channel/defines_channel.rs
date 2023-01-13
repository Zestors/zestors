use crate::*;
use std::{any::TypeId, sync::Arc};

//------------------------------------------------------------------------------------------------
//  DefinesChannel
//------------------------------------------------------------------------------------------------

/// A channel-definition is part of any [Address] or [Child], and defines the [Channel] used in the
/// actor and which messages the actor [trait@Accepts]. The different channel-definitions are:
/// - A [`Protocol`]. (sized)
/// - A [`Halter`]. (sized)
/// - A [`Dyn<_>`] type using the [`Accepts!`] macro. (dynamic)
pub trait DefinesChannel {
    type Channel: DynChannel + ?Sized;
    fn into_dyn_channel(channel: Arc<Self::Channel>) -> Arc<dyn DynChannel>;
}

impl<P: Protocol> DefinesChannel for P {
    type Channel = InboxChannel<P>;

    fn into_dyn_channel(channel: Arc<Self::Channel>) -> Arc<dyn DynChannel> {
        channel
    }
}

impl<D: ?Sized> DefinesChannel for Dyn<D> {
    type Channel = dyn DynChannel;

    fn into_dyn_channel(channel: Arc<Self::Channel>) -> Arc<dyn DynChannel> {
        channel
    }
}

impl DefinesChannel for Halter {
    type Channel = HalterChannel;

    fn into_dyn_channel(channel: Arc<Self::Channel>) -> Arc<dyn DynChannel> {
        channel
    }
}

/// A specialization of [`DefinesChannel`] for sized channels only.
pub trait DefinesSizedChannel: DefinesChannel
where
    Self::Channel: Sized,
{
    type Spawned: SpawnsWith<ChannelDefinition = Self>;
}

impl DefinesSizedChannel for Halter {
    type Spawned = Halter;
}

impl<P: Protocol> DefinesSizedChannel for P {
    type Spawned = Inbox<P>;
}

/// A specialization of [`DefinesChannel`] for dynamic channels only.
pub trait DefinesDynChannel: DefinesChannel<Channel = dyn DynChannel> {
    fn msg_ids() -> Box<[TypeId]>;
}

use crate::*;
use std::{any::TypeId, sync::Arc};

//------------------------------------------------------------------------------------------------
//  DefinesChannel
//------------------------------------------------------------------------------------------------

pub trait DefinesChannel {
    /// The underlying channel used.
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

//------------------------------------------------------------------------------------------------
//  DefinesSizedChannel
//------------------------------------------------------------------------------------------------

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

//------------------------------------------------------------------------------------------------
//  DefinesDynChannel
//------------------------------------------------------------------------------------------------

/// Trait implemented for all [`Dyn<...>`](Dyn) types, this never has to be implemented
/// manually.
pub trait DefinesDynChannel:
    DefinesChannel<Channel = dyn DynChannel>
{
    fn msg_ids() -> Box<[TypeId]>;
}

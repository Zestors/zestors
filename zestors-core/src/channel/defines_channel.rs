use crate::*;
use std::{any::TypeId, sync::Arc};

//------------------------------------------------------------------------------------------------
//  DefineChannel
//------------------------------------------------------------------------------------------------

/// A channel-definition is part of any [Address] or [Child], and defines the [Channel] used in the
/// actor and which messages the actor [Accept]. The different channel-definitions are:
/// - A [`Protocol`]. (sized)
/// - A [`Halter`]. (sized)
/// - A [`Dyn<_>`] type using the [`Accepts!`] macro. (dynamic)
pub trait DefineChannel {
    type Channel: DynChannel + ?Sized;
    fn into_dyn_channel(channel: Arc<Self::Channel>) -> Arc<dyn DynChannel>;
}



impl<D: ?Sized> DefineChannel for Dyn<D> {
    type Channel = dyn DynChannel;

    fn into_dyn_channel(channel: Arc<Self::Channel>) -> Arc<dyn DynChannel> {
        channel
    }
}



/// A specialization of [`DefineChannel`] for sized channels only.
pub trait DefineSizedChannel: DefineChannel
where
    Self::Channel: Sized,
{
    type Spawned: Spawn<ChannelDefinition = Self>;
}

/// A specialization of [`DefineChannel`] for dynamic channels only.
pub trait DefineDynChannel: DefineChannel<Channel = dyn DynChannel> {
    fn msg_ids() -> Box<[TypeId]>;
}

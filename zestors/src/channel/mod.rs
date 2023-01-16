mod address;
pub use address::*;

pub use zestors_core::channel::*;

pub trait ActorRefExt: ActorRef {
    fn get_address(&self) -> Address<Self::ActorKind> {
        let channel = <Self as ActorRef>::channel(&self).clone();
        channel.add_address();
        Address::from_channel(channel)
    }
    fn is_bounded(&self) -> bool {
        self.capacity().is_bounded()
    }
}

impl<T> ActorRefExt for T where T: ActorRef {}
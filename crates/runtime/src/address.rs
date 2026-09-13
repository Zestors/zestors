use crate::*;
use std::{fmt::Debug, hash::Hash};

#[repr(transparent)]
pub struct Address<C: Context = Dyn<()>> {
    pub(super) channel: Channel<C>,
}

impl<C: Context> Address<C> {
    pub(super) fn new(channel: &Channel<C>) -> Self {
        Self {
            channel: channel._clone(),
        }
    }

    pub(super) fn from_ref(channel: &Channel<C>) -> &Self {
        unsafe { std::mem::transmute::<&Channel<C>, &Address<C>>(channel) }
    }
}

impl<C: Context> ActorOps for Address<C> {
    type Ctx = C;

    fn handle(&self) -> &Channel<Self::Ctx> {
        &self.channel
    }
}

impl<C: Context> IntoDyn for Address<C> {
    type Ref<R: Context> = Address<R>;

    fn into_context_unchecked<R>(self) -> Self::Ref<R>
    where
        R: Context,
    {
        Address {
            channel: unsafe { std::mem::transmute::<Channel<Self::Ctx>, Channel<R>>(self.channel) },
        }
    }
}

impl<T: Context> Clone for Address<T> {
    fn clone(&self) -> Self {
        Self {
            channel: self.channel._clone(),
        }
    }
}

impl<T: Context> Debug for Address<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Address")
            .field("channel", &self.channel)
            .finish()
    }
}
impl<T: Context> PartialEq for Address<T> {
    fn eq(&self, other: &Self) -> bool {
        self.channel == other.channel
    }
}
impl<T: Context> Eq for Address<T> {}
impl<T: Context> Hash for Address<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.channel.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Interface)]
    #[interface(path = "zestors_interface")]
    pub enum MyInterface {
        A(Envelope<u32>),
        AB(Envelope<u64>),
    }

    #[tokio::test]
    async fn test_address_downcast_ref() {
        let child = crate::spawn(|_: Inbox<MyInterface>| async move { Ok(()) });
        let address = child.address().clone().into_dyn::<()>();

        address
            .downcast::<MyInterface>()
            .expect("Should downcast to MyInterface");
    }
}

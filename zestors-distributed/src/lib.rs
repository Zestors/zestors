use serde::{de::DeserializeOwned, Serialize};
use std::{
    any::TypeId,
    error::Error,
    io::{Read, Write},
    marker::PhantomData,
};
use zestors_core::{
    actor::Addr,
    actor_type::{Accepts, ActorType},
    protocol::{BoxedMessage, Message, Protocol, ProtocolMessage, Sends},
    *,
};

pub trait RemoteActorType: ActorType
where
    Self::Type: RemoteChannelType,
{
}

pub trait RemoteChannelType {}

struct RemoteAddress<T: ActorType>
where
    T::Type: RemoteChannelType,
{
    address: PhantomData<T>,
}

pub fn try_send_remote<T, RM>(t: &Addr<T>, msg: RM) -> ()
where
    RM: RemoteMessage,
    T: Accepts<RM::Message>,
{
    // <T as Accepts<M>>::try_send(&self.address, msg)
    todo!()
}

pub(crate) struct TestProt;
impl Protocol for TestProt {
    fn try_from_boxed(boxed: BoxedMessage) -> Result<Self, BoxedMessage> {
        todo!()
    }

    fn into_boxed(self) -> BoxedMessage {
        todo!()
    }

    fn accepts(id: &TypeId) -> bool {
        todo!()
    }
}

impl ProtocolMessage<u32> for TestProt {
    fn from_sends(msg: Sends<u32>) -> Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn try_into_sends(self) -> Result<Sends<u32>, Self>
    where
        Self: Sized,
    {
        todo!()
    }
}

impl ProtocolMessage<u64> for TestProt {
    fn from_sends(msg: Sends<u64>) -> Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn try_into_sends(self) -> Result<Sends<u64>, Self>
    where
        Self: Sized,
    {
        todo!()
    }
}

pub trait RemoteMessage {
    type Message: Message;

    fn size_hint(self) -> usize;
    fn serialize_into<W: Write>(self, writer: W);
    fn deserialize_from<R: Read>(reader: R) -> Result<Self::Message, Box<dyn Error>>;
}

impl<M> RemoteMessage for &M
where
    M: Message + Serialize + DeserializeOwned,
{
    type Message = M;

    fn size_hint(self) -> usize {
        bincode::serialized_size(self)
            .unwrap()
            .try_into()
            .unwrap_or(usize::MAX)
    }

    fn serialize_into<W: Write>(self, writer: W) {
        bincode::serialize_into(writer, self).unwrap();
    }

    fn deserialize_from<R: Read>(reader: R) -> Result<Self::Message, Box<dyn Error>> {
        Ok(bincode::deserialize_from(reader)?)
    }
}

fn test1(address: Addr<TestProt>, address2: DynAddress![u32, u64]) {
    address.try_send(10 as u32).unwrap();
    try_send_remote(&address, &(10 as u32));
    try_send_remote(&address, &(10 as u64));
    // address.try_send(10 as u128).unwrap();
    address2.try_send(10 as u32).unwrap();
    // address2.try_send(10 as u128).unwrap();
}

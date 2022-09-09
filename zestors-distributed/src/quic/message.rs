// use serde::{de::DeserializeOwned, Serialize};
// use std::{
//     any::{Any, TypeId},
//     error::Error,
//     io::{Read, Write},
//     marker::PhantomData,
// };
// use zestors_core::{
//     actor_type::Accepts,
//     process::Address,
//     protocol::{BoxedMessage, Message, MessageType, Protocol, ProtocolMessage, Sends},
//     *,
// };

// use crate::*;

// pub fn try_send_remote<T, RM>(t: &Address<T>, msg: RM) -> ()
// where
//     RM: AsRemoteMessage,
//     T: Accepts<RM::Message>,
// {
//     // <T as Accepts<M>>::try_send(&self.address, msg)
//     todo!()
// }

// pub(crate) struct TestProt;
// impl Protocol for TestProt {
//     fn try_from_boxed(boxed: BoxedMessage) -> Result<Self, BoxedMessage> {
//         todo!()
//     }

//     fn into_boxed(self) -> BoxedMessage {
//         todo!()
//     }

//     fn accepts(id: &TypeId) -> bool {
//         todo!()
//     }
// }

// impl ProtocolMessage<u32> for TestProt {
//     fn from_sends(msg: Sends<u32>) -> Self
//     where
//         Self: Sized,
//     {
//         todo!()
//     }

//     fn try_into_sends(self) -> Result<Sends<u32>, Self>
//     where
//         Self: Sized,
//     {
//         todo!()
//     }
// }

// impl ProtocolMessage<u64> for TestProt {
//     fn from_sends(msg: Sends<u64>) -> Self
//     where
//         Self: Sized,
//     {
//         todo!()
//     }

//     fn try_into_sends(self) -> Result<Sends<u64>, Self>
//     where
//         Self: Sized,
//     {
//         todo!()
//     }
// }

// pub type RemoteSends<M> = <<M as RemoteMessage<M>>::RemoteType as RemoteMessageType<M>>::RemoteReturns;

// pub trait RemoteMessage<M: AsRemoteMessage>: Message {
//     type RemoteType: RemoteMessageType<M>;
// }

// impl<M: AsRemoteMessage> RemoteMessage<M> for M::Message
// where
//     <M::Message as Message>::Type: RemoteMessageType<M>,
// {
//     type RemoteType = <M::Message as Message>::Type;
// }

// pub trait AsRemoteMessage: Sized {
//     type Message: RemoteMessage<Self>;

//     fn size_hint(&self) -> usize;
//     fn serialize_into<W: Write>(self, writer: W);
//     fn deserialize_from<R: Read>(reader: R) -> Self::Message;
// }

// pub trait RemoteMessageType<M: AsRemoteMessage>: MessageType<M::Message> {
//     type RemoteReturns;

//     fn register(msg: &M, system: &System) -> (Self::RemoteReturns, u64);
//     fn send_failed(msg: &M, returns: Self::Returns, system: &System);
//     fn create(msg: M::Message, node: &Node, id: u64) -> Self::Sends;
// }

// impl<M> AsRemoteMessage for &M
// where
//     M: RemoteMessage<Self> + Serialize + DeserializeOwned,
// {
//     type Message = M;

//     fn size_hint(&self) -> usize {
//         bincode::serialized_size(self)
//             .unwrap()
//             .try_into()
//             .unwrap_or(usize::MAX)
//     }

//     fn serialize_into<W: Write>(self, writer: W) {
//         bincode::serialize_into(writer, self).unwrap();
//     }

//     fn deserialize_from<R: Read>(reader: R) -> Self::Message {
//         bincode::deserialize_from(reader).unwrap()
//     }
// }

// fn test1(address: Address<TestProt>, address2: DynAddress![u32, u64]) {
//     address.try_send(10 as u32).unwrap();
//     try_send_remote(&address, &(10 as u32));
//     try_send_remote(&address, &(10 as u64));
//     // address.try_send(10 as u128).unwrap();
//     address2.try_send(10 as u32).unwrap();
//     // address2.try_send(10 as u128).unwrap();
// }

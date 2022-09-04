// use std::{any::TypeId, marker::PhantomData, sync::Arc};

// use zestors_core::{
//     actor_type::{Accepts, ActorType, Dyn},
//     process::Channel,
//     protocol::{BoxChannel, Message, Protocol},
//     DynAccepts,
// };

// use crate::{AsRemoteMessage, RemoteMessage, RemoteMessageType, RemoteSends};

// //------------------------------------------------------------------------------------------------
// //  RemoteAddress
// //------------------------------------------------------------------------------------------------

// pub struct RemoteAddress<T> {
//     msg_type: PhantomData<T>,
//     address: Arc<RemoteAddressInner>,
// }

// struct RemoteAddressInner {
//     protocol_type: TypeId,
//     send_stream: quinn::SendStream,
// }

// type RemoteReturns<M> = <<<M as AsRemoteMessage>::Message as RemoteMessage<M>>::RemoteType as RemoteMessageType<M>>::RemoteReturns;

// // enum RemoteTrySendError<M> {}

// // impl<T> RemoteAddress<T> {
// //     fn try_send<M>(&self, msg: M) -> Result<RemoteReturns<M>, RemoteTrySendError<M>>
// //     where
// //         T: Accepts<M::Message>,
// //         M: AsRemoteMessage,
// //     {
// //         let mut bytes = Vec::with_capacity(msg.size_hint());
// //         msg.serialize_into(&mut bytes);
// //         todo!()
// //     }
// // }

// impl<P: Protocol> RemoteAddress<P> {}

// impl<D> RemoteAddress<Dyn<D>> {}

// fn test() {
//     let a: RemoteAddress<DynAccepts![u32, u64]>;
//     let a: RemoteAddress<TestProt>;
// }

// struct TestProt;

// impl Protocol for TestProt {
//     fn try_from_boxed(
//         boxed: zestors_core::protocol::BoxedMessage,
//     ) -> Result<Self, zestors_core::protocol::BoxedMessage>
//     where
//         Self: Sized,
//     {
//         todo!()
//     }

//     fn into_boxed(self) -> zestors_core::protocol::BoxedMessage {
//         todo!()
//     }

//     fn accepts(id: &std::any::TypeId) -> bool
//     where
//         Self: Sized,
//     {
//         todo!()
//     }
// }

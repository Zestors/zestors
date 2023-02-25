// use std::marker::PhantomData;

// use futures::{future::BoxFuture, Future};

// use super::actor::{Actor, ActorState, Flow};

// pub struct Action<A: Actor, Fun>
// where
//     Fun: FnOnce(&mut A) -> BoxFuture<'_, Result<Flow, A::Error>>,
// {
//     function: Fun,
//     phantom: PhantomData<A>,
// }

// struct AsyncEvent<A, T, F, H, HF>
// where
//     A: Actor,
//     F: Future<Output = T>,
//     H: FnOnce(&mut A, T, &mut ActorState<A::InboxType>) -> HF,
//     HF: Future<Output = Result<Flow, A::Error>>,
// {
//     future: F,
//     handler: H,
//     p: PhantomData<A>,
// }

// struct SyncEvent<A, T, F, H>
// where
//     A: Actor,
//     F: Future<Output = T>,
//     H: FnOnce(&mut A, T, &mut ActorState<A::InboxType>) -> Result<Flow, A::Error>,
// {
//     future: F,
//     handler: H,
//     p: PhantomData<A>,
// }

// #[cfg(test)]
// mod test {
//     use crate::{
//         channel::{halter::Halter, inbox::Inbox},
//         monitoring::Address,
//         supervision::{
//             actor::{Actor, Flow, ExitFlow},
//             BoxError,
//         },
//     };

//     struct MyActor;

//     impl Actor for MyActor {
//         type InboxType = Inbox<Self::Protocol>;
//         type Protocol = ();
//         type Exit = ();
//         type Error = BoxError;
//         type Init = u32;
//         type Ref = Address<Halter>;

//         fn start<'async_trait>(
//             address: crate::monitoring::Address<Self::InboxType>,
//         ) -> core::pin::Pin<
//             Box<
//                 dyn core::future::Future<Output = Result<Self::Ref, crate::supervision::BoxError>>
//                     + core::marker::Send
//                     + 'async_trait,
//             >,
//         >
//         where
//             Self: 'async_trait,
//         {
//             todo!()
//         }

//         fn on_init<'life0, 'async_trait>(
//             init: Self::Init,
//             state: &'life0 mut crate::supervision::actor::ActorState<Self>,
//         ) -> core::pin::Pin<
//             Box<
//                 dyn core::future::Future<Output = Result<Self, Self::Exit>>
//                     + core::marker::Send
//                     + 'async_trait,
//             >,
//         >
//         where
//             'life0: 'async_trait,
//             Self: 'async_trait,
//         {
//             todo!()
//         }

//         fn handle_msg<'life0, 'life1, 'async_trait>(
//             &'life0 mut self,
//             msg: Self::Protocol,
//             state: &'life1 mut crate::supervision::actor::ActorState<Self>,
//         ) -> core::pin::Pin<
//             Box<
//                 dyn core::future::Future<
//                         Output = Result<crate::supervision::actor::Flow, Self::Error>,
//                     > + core::marker::Send
//                     + 'async_trait,
//             >,
//         >
//         where
//             'life0: 'async_trait,
//             'life1: 'async_trait,
//             Self: 'async_trait,
//         {
//             todo!()
//         }

//         fn on_halt<'life0, 'life1, 'async_trait>(
//             &'life0 mut self,
//             state: &'life1 mut crate::supervision::actor::ActorState<Self>,
//         ) -> core::pin::Pin<
//             Box<
//                 dyn core::future::Future<
//                         Output = Result<crate::supervision::actor::Flow, Self::Error>,
//                     > + core::marker::Send
//                     + 'async_trait,
//             >,
//         >
//         where
//             'life0: 'async_trait,
//             'life1: 'async_trait,
//             Self: 'async_trait,
//         {
//             todo!()
//         }

//         fn on_exit<'life0, 'async_trait>(
//             self,
//             err: Option<Self::Error>,
//             state: &'life0 mut crate::supervision::actor::ActorState<Self>,
//         ) -> core::pin::Pin<
//             Box<
//                 dyn core::future::Future<Output = ExitFlow<Self>>
//                     + core::marker::Send
//                     + 'async_trait,
//             >,
//         >
//         where
//             'life0: 'async_trait,
//             Self: 'async_trait,
//         {
//             todo!()
//         }
//     }
// }

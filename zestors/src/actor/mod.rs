// mod actor_state;
// pub use actor_state::*;

// use std::{collections::HashMap, error::Error, pin::Pin, sync::Arc};
// type BoxError = Box<dyn Error + Send>;

// use crate::{
//     all::Link,
//     channel::{
//         inbox::{HaltedError, Inbox},
//         ActorType, InboxType,
//     },
//     messaging::{Protocol, Tx},
//     monitoring::{ActorRef, ActorRefExt, Address, Child, ChildPool, ChildType},
//     spawning::spawn,
// };
// use async_trait::async_trait;
// use futures::{future::BoxFuture, stream::FuturesUnordered, Future, Stream};
// use zestors_codegen::{protocol, CustomHandler, Envelope, HandleStart, Message};

// pub trait ActorInbox:
//     InboxType + Stream<Item = Result<Self::Protocol, HaltedError>> + Unpin + ActorRef<ActorType = Self>
// {
//     type Protocol: Protocol;
// }

// impl<P: Protocol> ActorInbox for Inbox<P> {
//     type Protocol = P;
// }

// #[async_trait]
// pub trait HandleExit: Send + 'static + Sized {
//     type Inbox: ActorInbox;
//     type Exit: Send + 'static;

//     async fn handle_exit(
//         self,
//         exit: Result<(), BoxError>,
//         state: &mut DefaultState<Self>,
//     ) -> Result<Self::Exit, Self>;

//     async fn handle_halt(&mut self, state: &mut DefaultState<Self>) -> Result<Flow<Self>, BoxError>;
// }

// #[async_trait]
// pub trait HandleProtocol: HandleExit {
//     async fn handle_protocol(
//         &mut self,
//         protocol: <Self::Inbox as ActorInbox>::Protocol,
//         state: &mut DefaultState<Self>,
//     ) -> Result<Flow<Self>, BoxError>;
// }

// #[async_trait]
// pub trait HandleStart<Init>: HandleExit {
//     type Ref: Send + 'static;

//     async fn on_spawn(address: Address<Self::Inbox>) -> Result<Self::Ref, BoxError>;
//     async fn handle_start(init: Init, state: &mut DefaultState<Self>) -> Result<Self, Self::Exit>;
// }

// pub trait Actor<Init = Self>: HandleExit + HandleStart<Init> + HandleProtocol {
//     type Protocol: Protocol;

//     fn start_from(
//         init: Init,
//         link: Link,
//         config: <Self::Inbox as InboxType>::Config,
//     ) -> BoxFuture<
//         'static,
//         Result<
//             (
//                 Child<Self::Exit, Self::Inbox>,
//                 <Self as HandleStart<Init>>::Ref,
//             ),
//             BoxError,
//         >,
//     >
//     where
//         Init: Send + 'static,
//     {
//         let (child, address) = spawn(link, config, |inbox| async move {
//             let mut state = DefaultState::new(inbox);
//             let this = match Self::handle_start(init, &mut state).await {
//                 Ok(this) => this,
//                 Err(exit) => return exit,
//             };
//             state.run(this).await
//         });

//         Box::pin(async move {
//             match Self::on_spawn(address).await {
//                 Ok(reference) => Ok((child, reference)),
//                 Err(_) => todo!(),
//             }
//         })
//     }

//     fn start(
//         self,
//         link: Link,
//         config: <Self::Inbox as InboxType>::Config,
//     ) -> BoxFuture<
//         'static,
//         Result<
//             (
//                 Child<Self::Exit, Self::Inbox>,
//                 <Self as HandleStart<Self>>::Ref,
//             ),
//             BoxError,
//         >,
//     >
//     where
//         Self: HandleStart<Self>,
//     {
//         <Self as Actor<Self>>::start_from(self, link, config)
//     }
// }

// #[async_trait]
// pub trait HandleStartMany<Itm>: HandleExit {
//     type Init: Send + 'static;
//     type Ref: Send + 'static;

//     async fn on_spawn_many(address: Address<Self::Inbox>) -> Result<Self::Ref, BoxError>;

//     async fn handle_start_many(
//         init: Self::Init,
//         item: Itm,
//         state: &mut DefaultState<Self>,
//     ) -> Result<Self, Self::Exit>;
// }

// impl<T: HandleExit + HandleStart<Init> + HandleProtocol, Init> Actor<Init> for T {
//     type Protocol = <T::Inbox as ActorInbox>::Protocol;
// }

// pub trait PooledActor<Itm>: HandleExit + HandleProtocol + HandleStartMany<Itm> {
//     type Protocol: Protocol;

//     fn start_many(
//         self,
//         link: Link,
//         config: <Self::Inbox as InboxType>::Config,
//         iter: impl ExactSizeIterator<Item = Itm>,
//     ) -> BoxFuture<'static, Result<(ChildPool<Self::Exit, Self::Inbox>, Self::Ref), BoxError>>
//     where
//         Self: HandleStartMany<Itm, Init = Self>,
//     {
//         todo!()
//     }
// }

// pub enum Flow<A: HandleExit> {
//     Ok,
//     Exit(Option<A::Exit>),
// }

// pub enum ActorExit<A: HandleExit> {
//     Normal(Option<A::Exit>),
// }

// pub trait ActorExt: HandleExit {
//     fn spawn(self, link: Link, cfg: <Self::Inbox as InboxType>::Config) {
//         // spawn(function);
//     }
// }

// //------------------------------------------------------------------------------------------------
// //  Example of #[derive(Handler)]
// //------------------------------------------------------------------------------------------------

// use crate as zestors;

// #[derive(Message, Envelope)]
// #[request(&'static str)]
// #[envelope(MyMessageEnvelope, my_message)]
// struct MyMessage {
//     msg: &'static str,
// }

// #[protocol]
// #[derive(CustomHandler)]
// #[handler(MyProtocolHandler)]
// enum MyProtocol {
//     #[handler(handle_a)]
//     A(u32),
//     #[handler(handle_b)]
//     B(MyMessage),
// }

// #[derive(HandleStart)]
// struct MyActor;

// #[async_trait]
// impl HandleExit for MyActor {
//     type Inbox = Inbox<MyProtocol>;
//     type Exit = String;

//     async fn handle_exit(
//         self,
//         exit: Result<(), BoxError>,
//         state: &mut DefaultState<Self>,
//     ) -> Result<Self::Exit, Self> {
//         Ok("exit".to_string())
//     }

//     async fn handle_halt(&mut self, state: &mut DefaultState<Self>) -> Result<Flow<Self>, BoxError> {
//         state.close();
//         Ok(Flow::Exit(None))
//     }
// }

// #[async_trait]
// impl HandleMyProtocol for MyActor {
//     async fn handle_a(
//         &mut self,
//         msg: u32,
//         state: &mut DefaultState<Self>,
//     ) -> Result<Flow<Self>, BoxError> {
//         todo!()
//     }

//     async fn handle_b(
//         &mut self,
//         msg: (MyMessage, Tx<&'static str>),
//         state: &mut DefaultState<Self>,
//     ) -> Result<Flow<Self>, BoxError> {
//         todo!()
//     }
// }

// async fn test() {
//     let (child, address) = MyActor
//         .start(Default::default(), Default::default())
//         .await
//         .unwrap();

//     MyActor::start(MyActor, Default::default(), Default::default());
// }

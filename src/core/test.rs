// use std::{any::Any, error::Error, marker::PhantomData, pin::Pin};

// use futures::Stream;

// use crate::core::*;

// //------------------------------------------------------------------------------------------------
// //  ::::::: TESTING ::::: EchoAddr trait
// //------------------------------------------------------------------------------------------------

// pub trait EchoAddr: Address {
//     /// Automatically generated trait function. The signature is as follows:
//     /// ```ignore
//     /// echo(msg1: String, msg2: u32) -> String
//     /// ```
//     fn echo(
//         &self,
//         msg1: <Self::Loc as Location>::Msg<'static, String>,
//         msg2: <Self::Loc as Location>::Msg<'static, u32>,
//     ) -> <Self::Loc as Location>::Result<(String, u32), String>;
// }

// //------------------------------------------------------------------------------------------------
// //  MyAddr
// //------------------------------------------------------------------------------------------------

// #[derive(Clone)]
// struct MyActorAddr<Loc: Location> {
//     loc: PhantomData<Loc>,
//     addr: Addr<Loc::Addr<Self>>,
// }

// impl Address for MyActorAddr<Remote> {
//     fn into_any(self: Box<Self>) -> Box<dyn Any> {
//         self
//     }
//     fn as_any(&self) -> &dyn Any {
//         self
//     }
//     fn as_mut_any(&mut self) -> &mut dyn Any {
//         self
//     }

//     type Loc = Remote;
// }

// impl Addressable for MyActorAddr<Remote> {
//     type Actor = MyActor;

//     fn from_addr(addr: Addr<MyActor>) -> Self {
//         Self {
//             loc: PhantomData,
//             addr,
//         }
//     }
//     fn as_addr(&self) -> &Addr<MyActor> {
//         &self.addr
//     }
// }

// impl EchoAddr for  MyActorAddr<Remote> {
//     fn echo(
//         &self,
//         msg1: <Self::Loc as Location>::Msg<'_, String>,
//         msg2: <Self::Loc as Location>::Msg<'_, u32>,
//     ) -> <Self::Loc as Location>::Result<(String, u32), String> {
//         // let p: &String = params;
//         Box::pin(async move { todo!() })
//     }
// }

// impl Address for MyAddr {
//     fn into_any(self: Box<Self>) -> Box<dyn Any> {
//         self
//     }
//     fn as_any(&self) -> &dyn Any {
//         self
//     }
//     fn as_mut_any(&mut self) -> &mut dyn Any {
//         self
//     }

//     type Loc = Local;
// }

// impl Addressable for MyAddr {
//     type Actor = MyActor;

//     fn from_addr(addr: Addr<MyActor>) -> Self {
//         Self(addr)
//     }
//     fn as_addr(&self) -> &Addr<MyActor> {
//         &self.0
//     }
// }

// impl EchoAddr for MyAddr {
//     fn echo(
//         &self,
//         msg1: <Self::Loc as Location>::Msg<'static, String>,
//         msg2: <Self::Loc as Location>::Msg<'static, u32>,
//     ) -> <Self::Loc as Location>::Result<(String, u32), String> {
//         let (req, reply) = Sender::new();
//         Ok(reply)
//     }
// }

// // #[tokio::test]
// async fn test() {
//     let addr = MyAddr(todo!());

//     let remote_addr = MyRemoteAddr(todo!());

//     let boxed: Box<dyn EchoAddr<Loc = Local>> = Box::new(addr);
//     let reply: String = boxed.echo("hi".to_string(), 10).unwrap().await.unwrap();
//     let addr = *boxed.downcast::<MyAddr>().unwrap();
//     let res: String = addr.echo("hi".to_string(), 10).unwrap().await.unwrap();

//     let boxed: Box<dyn EchoAddr<Loc = Remote>> = Box::new(remote_addr);
//     let reply: String = boxed
//         .echo(&"hi".to_string(), &10)
//         .await
//         .unwrap()
//         .await
//         .unwrap();
// }

// //------------------------------------------------------------------------------------------------
// //  MyActor
// //------------------------------------------------------------------------------------------------

// struct MyActor;

// impl Stream for MyActor {
//     type Item = Action<Self>;
//     fn poll_next(
//         self: Pin<&mut Self>,
//         cx: &mut std::task::Context<'_>,
//     ) -> std::task::Poll<Option<Self::Item>> {
//         todo!()
//     }
// }

// impl Actor for MyActor {
//     type Init = ();
//     type Error = Box<dyn Send + Error>;
//     type Exit = Self;
//     type Addr = MyAddr;

//     fn init(init: Self::Init, address: Self::Addr) -> InitFlow<Self> {
//         InitFlow::Init(Self)
//     }

//     fn handle_signal(self, signal: Signal<Self>, state: State<Self>) -> SignalFlow<Self> {
//         SignalFlow::Exit(self)
//     }
// }

// struct TestActor<T = Local>(T);
// fn test2(test: TestActor<Remote>) {}

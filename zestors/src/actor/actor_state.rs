// use super::*;

// pub trait ActorState {
//     type Protocol: Protocol;
// }

// pub struct DefaultState<A: HandleExit> {
//     inbox: A::Inbox,
//     next_stream: u64,
//     streams: Vec<(
//         u64,
//         Pin<Box<dyn Stream<Item = Option<<A::Inbox as ActorInbox>::Protocol>> + Send + 'static>>,
//     )>,
//     next_future: u64,
//     futures: Vec<(
//         u64,
//         Pin<Box<dyn Future<Output = Option<<A::Inbox as ActorInbox>::Protocol>> + Send + 'static>>,
//     )>,
// }

// impl<A: HandleExit> std::fmt::Debug for DefaultState<A> {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("ActorState").finish()
//     }
// }

// impl<A: HandleExit> DefaultState<A> {
//     pub fn new(inbox: A::Inbox) -> Self {
//         DefaultState {
//             inbox,
//             streams: Vec::new(),
//             futures: Vec::new(),
//             next_stream: 0,
//             next_future: 0,
//         }
//     }

//     pub async fn run(self, actor: A) -> A::Exit {
//         todo!()
//     }

//     pub fn schedule_future(
//         &mut self,
//         future: impl Future<Output = Option<<A::Inbox as ActorInbox>::Protocol>> + Send + 'static,
//     ) -> u64 {
//         let id = self.next_future;
//         self.next_future += 1;
//         self.futures.push((id, Box::pin(future)));
//         id
//     }

//     pub fn remove_future(
//         &mut self,
//         id: u64,
//     ) -> Option<
//         Pin<Box<dyn Future<Output = Option<<A::Inbox as ActorInbox>::Protocol>> + Send + 'static>>,
//     > {
//         self.futures
//             .iter()
//             .enumerate()
//             .find_map(
//                 |(index, (this_id, _))| {
//                     if *this_id == id {
//                         Some(index)
//                     } else {
//                         None
//                     }
//                 },
//             )
//             .map(|index| self.futures.swap_remove(index).1)
//     }

//     pub fn schedule_stream(
//         &mut self,
//         stream: impl Stream<Item = Option<<A::Inbox as ActorInbox>::Protocol>> + Send + 'static,
//     ) -> u64 {
//         let id = self.next_stream;
//         self.next_stream += 1;
//         self.streams.push((id, Box::pin(stream)));
//         id
//     }

//     pub fn remove_stream(
//         &mut self,
//         id: u64,
//     ) -> Option<
//         Pin<Box<dyn Stream<Item = Option<<A::Inbox as ActorInbox>::Protocol>> + Send + 'static>>,
//     > {
//         self.streams
//             .iter()
//             .enumerate()
//             .find_map(
//                 |(index, (this_id, _))| {
//                     if *this_id == id {
//                         Some(index)
//                     } else {
//                         None
//                     }
//                 },
//             )
//             .map(|index| self.streams.swap_remove(index).1)
//     }
// }

// impl<A: HandleExit> ActorRef for DefaultState<A> {
//     type ActorType = A::Inbox;

//     fn channel_ref(&self) -> &Arc<<Self::ActorType as ActorType>::Channel> {
//         self.inbox.channel_ref()
//     }
// }

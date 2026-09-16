use std::{convert::Infallible, fmt::Debug};

use zestors::actor::{Actor, Blueprint};

pub struct ServerBlueprint {
    
}

// pub struct AxumServerBlueprint<A: axum_server::Address> {
//     server: axum_server::Server<A>,
// }

// impl<A: axum_server::Address> AxumServerBlueprint<A> {
//     pub fn new(server: axum_server::Server<A>) -> Self {
//         Self { server }
//     }
// }

// impl<A: axum_server::Address> Debug for AxumServerBlueprint<A> {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("AxumServerBlueprint").finish()
//     }
// }

// impl<A: axum_server::Address> Blueprint for AxumServerBlueprint<A> {
//     type Actor = Self;

//     async fn instantiate(&self) -> rootcause::Result<Self::Actor> {
//         Ok(self)
//     }
// }

// impl<A: axum_server::Address> Actor for AxumServerBlueprint<A> {
//     type Interface = Infallible;
//     type Exit = ();

//     fn run(
//         self,
//         inbox: zestors::prelude::Inbox<Self::Interface>,
//     ) -> impl Future<Output = Result<Self::Exit, Report>> + Send + 'static {
//         todo!()
//     }
// }

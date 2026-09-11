use rootcause::Report;
use std::{net::SocketAddr, pin::pin, sync::Arc};
use tokio::net::TcpListener;
use zestors_actor::{Actor, Blueprint};
use zestors_codegen::Interface;
use zestors_interface::Envelope;
use zestors_runtime::prelude::*;
use zestors_supervision::{GetChildren, GetHealth, Health};

mod router;

#[derive(Clone, Debug)]
pub struct ApiServerBlueprint {
    pub addr: SocketAddr,
}

impl Blueprint for ApiServerBlueprint {
    type Actor = ApiServer;

    async fn instantiate(&self) -> rootcause::Result<Self::Actor> {
        Ok(ApiServer::new(self.clone()))
    }
}

impl ApiServerBlueprint {
    pub fn new(addr: SocketAddr) -> Self {
        Self { addr }
    }
}

#[derive(Clone, Debug)]
pub struct ApiServer {
    cfg: Arc<ApiServerBlueprint>,
}

#[derive(Interface)]
#[interface(path = "zestors_interface")]
pub enum ApiServerInterface {
    Children(Envelope<GetChildren>),
    Health(Envelope<GetHealth>),
}

impl Actor for ApiServer {
    type Interface = ApiServerInterface;
    type Exit = ();

    async fn run(self, mut state: Inbox<Self::Interface>) -> Result<Self::Exit, Report> {
        let mut run_api = pin!(self.clone().run());

        loop {
            let event = tokio::select! {
                api_exit = &mut run_api => {
                    match &api_exit {
                        Ok(_) => {
                            tracing::info!("API server exited gracefully");
                        }
                        Err(e) => {
                            tracing::error!("API server exited with error: {e}");
                        }
                    }
                    break api_exit.map_err(Into::into);
                },

                event = state.next() => match event {
                    Some(event) => event,
                    None => break Err(rootcause::report!("Actor event stream closed unexpectedly")),
                }
            };

            match event {
                InboxEvent::Signal(signal) => match signal {
                    Signal::Shutdown => {
                        tracing::info!("API server received shutdown signal");
                        break Ok(());
                    }
                    Signal::Resume | Signal::Suspend => {}
                },

                InboxEvent::Message(msg) => match msg {
                    ApiServerInterface::Children(env) => {
                        env.handle.reply(vec![]).ok();
                    }
                    ApiServerInterface::Health(env) => {
                        env.handle.reply(Health::healthy()).ok();
                    }
                },
            }
        }
    }
}

impl ApiServer {
    fn new(cfg: ApiServerBlueprint) -> Self {
        Self { cfg: Arc::new(cfg) }
    }

    pub fn blueprint(addr: SocketAddr) -> ApiServerBlueprint {
        ApiServerBlueprint::new(addr)
    }

    async fn run(self) -> Result<(), Report> {
        let router = self.create_router();

        let listener = TcpListener::bind(self.cfg.addr).await?;

        tracing::info!("API server running at http://{}", self.cfg.addr);

        axum::serve(listener, router.into_make_service()).await?;

        Ok(())
    }
}

pub mod prelude {}

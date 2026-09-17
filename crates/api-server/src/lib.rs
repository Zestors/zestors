//! An HTTP API server actor for `zestors`: exposes introspection endpoints
//! over a running supervision tree.
//!
//! [`ApiServerBlueprint`] builds an [`ApiServer`] actor that binds an `axum`
//! server to a socket address. The HTTP endpoints are unstable, and will change
//! with minor version bumps.
//!
//! The server discovers its root supervisor from
//! [`ApiServerBlueprint::root_supervisor_pid`] (falling back to the actor's
//! parent [`Pid`]) and walks the tree via the child/health query messages from
//! `zestors-supervision`.

use rootcause::Report;
use std::{net::SocketAddr, pin::pin, sync::Arc};
use tokio::net::TcpListener;
use zestors_actor::{Actor, ActorBlueprint};
use zestors_codegen::Interface;
use zestors_interface::Envelope;
use zestors_runtime::{Registry, prelude::*};
use zestors_supervision::messages::{GetChildren, GetHealth, Health};

mod router;

/// A reusable recipe for an [`ApiServer`]: the socket address to bind, and
/// optionally the [`Pid`] of the root supervisor to introspect.
///
/// Build one with [`ApiServerBlueprint::new`] or [`ApiServer::blueprint`], then
/// spawn it like any other blueprint.
#[derive(Clone, Debug)]
pub struct ApiServerBlueprint {
    /// The socket address to bind the HTTP server to.
    pub addr: SocketAddr,
    /// The [`Pid`] of the root supervisor; `None` falls back to the actor's
    /// parent.
    pub root_supervisor_pid: Option<Pid>,
}

impl ActorBlueprint for ApiServerBlueprint {
    type Actor = ApiServer;

    async fn instantiate(&self) -> rootcause::Result<Self::Actor> {
        Ok(ApiServer::build(self.clone())?)
    }
}

impl ApiServerBlueprint {
    /// Creates a blueprint for an [`ApiServer`] bound to `addr`, with no
    /// explicit root supervisor (the actor's parent is used).
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            root_supervisor_pid: None,
        }
    }

    /// Sets the [`Pid`] of the root supervisor to introspect.
    pub fn root_supervisor_pid(mut self, pid: impl Into<Pid>) -> Self {
        self.root_supervisor_pid = Some(pid.into());
        self
    }
}

/// An actor that runs an `axum` HTTP server exposing supervision-tree
/// introspection endpoints. See the crate-level documentation for the
/// available routes.
#[derive(Clone, Debug)]
pub struct ApiServer {
    cfg: Arc<ApiServerBlueprint>,
    root_supervisor: Pid,
}

/// The message interface an [`ApiServer`] accepts: the [`GetChildren`] and
/// [`GetHealth`] queries from `zestors-supervision`.
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

                event = state.recv_event() => match event {
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
                        env.req.reply(vec![]).ok();
                    }
                    ApiServerInterface::Health(env) => {
                        env.req.reply(Health::healthy()).ok();
                    }
                },
            }
        }
    }
}

impl ApiServer {
    fn build(cfg: ApiServerBlueprint) -> Result<Self, Report> {
        let root_supervisor = cfg
            .root_supervisor_pid
            .clone()
            .or_else(|| Pid::parent())
            .ok_or_else(|| rootcause::report!("No root supervisor PID found"))?;

        Registry::local()
            .get(&root_supervisor)
            .ok_or_else(|| rootcause::report!("Root supervisor not found"))?;

        Ok(Self {
            cfg: Arc::new(cfg),
            root_supervisor,
        })
    }

    /// Creates an [`ApiServerBlueprint`] bound to `addr`.
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

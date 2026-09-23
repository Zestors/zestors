//! An HTTP API server actor for `zestors`: exposes introspection endpoints
//! over a running supervision tree.
//!
//! [`ApiServer::blueprint`] builds an [`ApiServer`] actor that binds an `axum`
//! server to a socket address, and serves the tree below the root supervisor
//! with the [`Name`] it is given. It walks the tree with the `GetChildren` and
//! `GetHealth` queries from `zestors-supervision`, so it sees every actor that
//! accepts them, whatever its type.
//!
//! | Route | Returns |
//! |---|---|
//! | `GET /processes` | every actor in the tree, with its status and child configuration |
//! | `GET /snapshots` | a `ChannelSnapshot` per name, or `null` if it is gone |
//! | `GET /health` | a `Health` per name, or `null` if it is gone or didn't answer in time |
//!
//! `/snapshots` and `/health` take a JSON array of names as the request body.
//! The routes are unstable, and may change with any minor version.
//!
//! The server sees the actors of one process; in a cluster, each node runs its
//! own. The `zestors-inspector` GUI draws what it serves.

use rootcause::Report;
use std::{net::SocketAddr, pin::pin, sync::Arc};
use tokio::net::TcpListener;
use zestors_actor::{Actor, Blueprint};
use zestors_codegen::Interface;
use zestors_interface::Envelope;
use zestors_runtime::{Registry, prelude::*};
use zestors_supervision::messages::{GetChildren, GetHealth, Health};

mod router;

/// A reusable recipe for an [`ApiServer`]: the socket address to bind, and the
/// [`Name`] of the root supervisor to introspect.
///
/// Build one with [`ApiServerBlueprint::new`] or [`ApiServer::blueprint`], then
/// spawn it like any other blueprint.
#[derive(Clone, Debug)]
pub struct ApiServerBlueprint {
    /// The socket address to bind the HTTP server to.
    addr: SocketAddr,
    /// The [`Name`] of the root supervisor. It has to be registered when the
    /// server starts.
    root_supervisor_name: Name,
}

impl Blueprint for ApiServerBlueprint {
    type Actor = ApiServer;

    async fn instantiate(&self) -> rootcause::Result<Self::Actor> {
        Ok(ApiServer::build(self.clone())?)
    }
}

impl ApiServerBlueprint {
    /// Creates a blueprint for an [`ApiServer`] bound to `addr`, serving the
    /// tree below the supervisor named `root_supervisor_name`.
    pub fn new(addr: SocketAddr, root_supervisor_name: impl Into<Name>) -> Self {
        Self {
            addr,
            root_supervisor_name: root_supervisor_name.into(),
        }
    }
}

/// An actor that runs an `axum` HTTP server exposing supervision-tree
/// introspection endpoints. The routes are listed in the
/// [crate documentation](crate).
#[derive(Clone, Debug)]
pub struct ApiServer {
    cfg: Arc<ApiServerBlueprint>,
    root_supervisor: Name,
}

/// The message interface an [`ApiServer`] accepts: the [`GetChildren`] and
/// [`GetHealth`] queries from `zestors-supervision`.
#[derive(Interface)]
#[zestors(interface_path = "zestors_interface")]
pub enum ApiServerInterface {
    /// Answered with no children: the server supervises nothing.
    Children(Envelope<GetChildren>),
    /// Answered with healthy.
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
        let root_supervisor = cfg.root_supervisor_name.clone();

        Registry::local()
            .get(&root_supervisor)
            .ok_or_else(|| rootcause::report!("Root supervisor not found"))?;

        Ok(Self {
            cfg: Arc::new(cfg),
            root_supervisor,
        })
    }

    /// Creates an [`ApiServerBlueprint`] bound to `addr`, and will expose the supervision-tree starting
    /// from the specified `root_supervisor_name`.
    pub fn blueprint(
        addr: SocketAddr,
        root_supervisor_name: impl Into<Name>,
    ) -> ApiServerBlueprint {
        ApiServerBlueprint::new(addr, root_supervisor_name)
    }

    async fn run(self) -> Result<(), Report> {
        let router = self.create_router();

        let listener = TcpListener::bind(self.cfg.addr).await?;

        tracing::info!("API server running at http://{}", self.cfg.addr);

        axum::serve(listener, router.into_make_service()).await?;

        Ok(())
    }
}

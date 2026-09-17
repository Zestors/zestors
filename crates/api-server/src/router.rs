use axum::{Json, Router, extract::State};
use axum_error_sets::{ApiResult, codes::Internal};
use axum_typed_routing::{TypedRouter, route};
use futures::{StreamExt, stream};
use indexmap::IndexMap;
use std::time::Duration;
use zestors_runtime::{ActorStatus, CallOptions, ChannelSnapshot, Context, Registry, prelude::*};
use zestors_supervision::{
    ChildConfig, ChildDescription,
    messages::{GetChildren, GetHealth, Health},
};

impl ApiServer {
    /// Builds the `axum` [`Router`] with this server's endpoints.
    pub(super) fn create_router(&self) -> Router {
        Router::new()
            .typed_route(get_processes)
            .typed_route(get_channel_snapshots)
            .typed_route(get_health)
            .with_state(self.clone())
    }
}

/// Returns all processes in the tree, with their actor-status and child-configuration
#[route(GET "/processes")]
#[axum::debug_handler]
async fn get_processes(
    State(state): State<ApiServer>,
) -> ApiResult<Json<IndexMap<Pid, (ChildConfig, ActorStatus, Vec<Pid>)>>, (Internal<String>,)> {
    // let mut addresses = Registry::local().fetch_addresses().await;

    // stream::iter(addresses.drain(..)).buffered(10);

    let root_pid = state.root_supervisor.clone();
    let root_desc = ChildDescription {
        pid: root_pid,
        cfg: ChildConfig::default(),
    };
    let root_address = Registry::local()
        .get(&root_desc.pid)
        .ok_or_else(|| Internal("Root supervisor not found in registry".to_string()))?;

    let mut pending = Vec::from_iter([(root_address, root_desc)]);
    let mut results = IndexMap::new();

    while !pending.is_empty() {
        let new_children = stream::iter(pending.drain(..))
            .map(|(address, desc)| async move {
                let children = get_children(&address).await.unwrap_or_default();
                ((address, desc), children)
            })
            .buffered(10)
            .collect::<Vec<_>>()
            .await;

        for ((address, desc), children) in new_children {
            let child_pids = children.iter().map(|c| c.pid.clone()).collect();
            let existing = results.insert(desc.pid, (desc.cfg, address.status(), child_pids));

            if let Some(duplicate_process) = &existing {
                return Err(Internal(format!("Supervision tree is circular or changed during traversal. Circle contains {duplicate_process:?}")).into());
            }

            for child in children {
                let Some(child_address) = Registry::local().get(&child.pid) else {
                    continue;
                };

                pending.push((child_address, child));
            }
        }
    }

    Ok(Json(results))
}

async fn get_children(address: &Address<impl Context>) -> rootcause::Result<Vec<ChildDescription>> {
    Ok(timeout(
        Duration::from_millis(50),
        address.call_dyn_with(GetChildren, CallOptions::new().ignore_exiting(true)),
    )
    .await??)
}

/// Returns a [`ChannelSnapshot`] for each requested [`Pid`], or `None` if it
/// is no longer registered.
#[route(GET "/snapshots" with ApiServer)]
async fn get_channel_snapshots(
    Json(pids): Json<Vec<Pid>>,
) -> ApiResult<Json<Vec<Option<ChannelSnapshot>>>, ()> {
    let results = pids
        .into_iter()
        .map(|pid| {
            Registry::local()
                .get(&pid)
                .map(|address| address.snapshot())
        })
        .collect::<Vec<_>>();

    Ok(Json(results))
}

/// Returns the [`Health`] of each requested [`Pid`], or `None` if it is no
/// longer registered or fails to respond in time.
#[route(GET "/health" with ApiServer)]
async fn get_health(Json(pids): Json<Vec<Pid>>) -> ApiResult<Json<Vec<Option<Health>>>, ()> {
    let results = stream::iter(pids.into_iter().map(|pid| async move {
        let Some(address) = Registry::local().get(&pid) else {
            return None;
        };

        match timeout(
            Duration::from_millis(50),
            address.call_dyn_with(GetHealth, CallOptions::new().ignore_exiting(true)),
        )
        .await
        {
            Ok(Ok(health)) => Some(health),
            _ => None,
        }
    }))
    .buffered(10)
    .collect::<Vec<_>>()
    .await;

    Ok(Json(results))
}

use tokio::time::timeout;

use crate::ApiServer;

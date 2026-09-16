use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use axum_typed_routing::{TypedRouter, route};
use futures::{StreamExt, stream};
use indexmap::IndexMap;
use rootcause::report;
use std::time::Duration;
use zestors_runtime::{ActorStatus, CastOptions, ChannelSnapshot, Context, Registry, prelude::*};
use zestors_supervision::{ChildConfig, ChildDescription, GetChildren, GetHealth, Health, Node};

impl ApiServer {
    pub(super) fn create_router(&self) -> Router {
        Router::new()
            .typed_route(get_tree)
            .typed_route(get_processes)
            .typed_route(get_channel_snapshots)
            .typed_route(get_health)
            .with_state(self.clone())
    }
}

#[route(GET "/tree?pid&include_debug" with ApiServer)]
async fn get_tree(pid: Option<Pid>, include_debug: Option<bool>) -> ApiResult {
    tracing::debug!("Received request for supervision with {pid:?} and {include_debug:?}");

    // let include_debug = include_debug.unwrap_or(false);

    // let pid = pid
    //     .or_else(|| Node::root_supervisor().map(|desc| desc.pid.clone()))
    //     .ok_or_else(|| report!("No PID provided and no root supervisor PID found"))?;

    // let tree = SupervisionTree::new(ChildDescription {
    //     pid,
    //     cfg: ChildConfig {
    //         restart_mode: RestartMode::Always,
    //         abort_timeout: Duration::from_secs(10),
    //         init_timeout: Duration::from_secs(10),
    //     },
    // })
    // .populated(Duration::from_millis(50))
    // .await
    // .populated_channel_snapshots();

    // let tree = if include_debug {
    //     tree.populated_debug_state(Duration::from_millis(50)).await
    // } else {
    //     tree
    // };

    // Ok((StatusCode::OK, Json(tree)).into_response())
    unimplemented!()
}

/// Returns all processes in the tree, with their actor-status and child-configuration
#[route(GET "/processes")]
#[axum::debug_handler]
async fn get_processes(
    State(state): State<ApiServer>,
) -> ApiResult<Json<IndexMap<Pid, (ChildConfig, ActorStatus, Vec<Pid>)>>> {
    let root_pid = state.root_supervisor.clone();
    let root_desc = ChildDescription {
        pid: root_pid,
        cfg: ChildConfig::default(),
    };
    let root_address = Registry::local()
        .get(&root_desc.pid)
        .ok_or_else(|| report!("Root supervisor not found in registry"))?;

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
                return Err(report!("Supervision tree is circular or changed during traversal. Circle contains {duplicate_process:?}").into());
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
        address.call_dyn_with(GetChildren, CastOptions::new().ignore_exiting(true)),
    )
    .await??)
}

#[route(GET "/snapshots" with ApiServer)]
async fn get_channel_snapshots(
    Json(pids): Json<Vec<Pid>>,
) -> ApiResult<Json<Vec<Option<ChannelSnapshot>>>> {
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

#[route(GET "/health" with ApiServer)]
async fn get_health(Json(pids): Json<Vec<Pid>>) -> ApiResult<Json<Vec<Option<Health>>>> {
    let results = stream::iter(pids.into_iter().map(|pid| async move {
        let Some(address) = Registry::local().get(&pid) else {
            return None;
        };

        match timeout(
            Duration::from_millis(50),
            address.call_dyn_with(GetHealth, CastOptions::new().ignore_exiting(true)),
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

use error::*;
use tokio::time::timeout;

use crate::ApiServer;
mod error;

use indexmap::IndexMap;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::sync::mpsc;
use zestors::{
    runtime::{ActorStatus, ChannelSnapshot, Name},
    supervision::ChildConfig,
};

use crate::api_client::Client;

static CLIENT: LazyLock<Client> =
    LazyLock::new(|| Client::new("http://localhost:8080").expect("Failed to create API client"));

const SLEEP_INTERVAL: Duration = Duration::from_millis(500);

pub enum ApiMessage {
    ProcessesUpdate(rootcause::Result<IndexMap<Name, (ChildConfig, ActorStatus, Vec<Name>)>>),
    NewChannelSnapshots(rootcause::Result<Vec<Option<ChannelSnapshot>>>),
}

pub async fn run_tree_poller(sender: mpsc::Sender<ApiMessage>, ctx: egui::Context) {
    loop {
        let processes = CLIENT.get_processes().await;

        sender
            .send(ApiMessage::ProcessesUpdate(processes))
            .await
            .ok();

        ctx.request_repaint();
        tokio::time::sleep(SLEEP_INTERVAL).await;
    }
}

pub fn update_channel_snapshots(
    sender: mpsc::Sender<ApiMessage>,
    names: Vec<Name>,
    ctx: egui::Context,
) {
    tokio::spawn(async move {
        let snapshots = CLIENT.get_channel_snapshots(names).await;

        sender
            .send(ApiMessage::NewChannelSnapshots(snapshots))
            .await
            .ok();

        ctx.request_repaint();
    });
}

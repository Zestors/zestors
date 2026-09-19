use crate::components::SupervisionNodeWidget;
use crate::{api::ApiMessage, app::process_map::ProcessMap};
use egui::{Color32, RichText};
use indexmap::IndexMap;
use std::collections::HashSet;
use tokio::{sync::mpsc, time::Instant};
use zestors::{
    runtime::{ActorStatus, ChannelSnapshot, Name},
    supervision::ChildConfig,
};

pub struct MyApp {
    pub sender: mpsc::Sender<ApiMessage>,
    pub receiver: mpsc::Receiver<ApiMessage>,
    pub map: ProcessMap,
    pub error_message: Option<String>,
}

impl Default for MyApp {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel(1_000_000);
        Self {
            sender: tx,
            receiver: rx,
            map: ProcessMap::default(),
            error_message: None,
        }
    }
}

impl MyApp {
    fn handle_incoming_messages(&mut self) {
        while let Ok(msg) = self.receiver.try_recv() {
            match msg {
                ApiMessage::ProcessesUpdate(processes) => match processes {
                    Ok(processes) => {
                        self.error_message = None;
                        self.map.merge(processes);
                    }
                    Err(e) => {
                        self.error_message = Some(format!("Error updating processes: {:#?}", e));
                    }
                },
                ApiMessage::NewChannelSnapshots(channel_snapshots) => match channel_snapshots {
                    Ok(snapshots) => {
                        self.error_message = None;
                        self.map.add_snapshots(snapshots);
                    }
                    Err(e) => {
                        self.error_message =
                            Some(format!("Error updating channel snapshots: {:#?}", e));
                    }
                },
            }
        }
    }

    fn render_header(&self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.heading(
            RichText::new("⚡ Zestors Inspector")
                .strong()
                .color(Color32::from_rgb(220, 225, 240)),
        );
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_incoming_messages();

        egui::CentralPanel::default().show(ui, |ui| {
            self.render_header(ui);
            if let Some(error) = &self.error_message {
                ui.colored_label(Color32::from_rgb(255, 100, 100), error);
                ui.add_space(4.0);
            }

            let tree = self.map.tree();
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let ctx = ui.ctx().clone();
                    for tree in &tree {
                        SupervisionNodeWidget::new(tree, true, &mut |name| {
                            crate::api::update_channel_snapshots(
                                self.sender.clone(),
                                vec![name.clone()],
                                ctx.clone(),
                            );
                        })
                        .show(ui);
                        ui.add_space(4.0);
                    }
                });
        });
    }
}

mod process_map;
pub use process_map::*;

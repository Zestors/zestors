#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod api_client;
mod app;
mod components;
mod theme;
mod utils;

use app::MyApp;
use theme::Theme;

fn main() -> eframe::Result {
    env_logger::init();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    std::thread::spawn(move || {
        rt.block_on(futures::future::pending::<()>());
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([500.0, 600.0]),
        ..Default::default()
    };

    let app = MyApp::default();

    eframe::run_native(
        "Zestors Inspector",
        options,
        Box::new(|cc| {
            Theme::apply(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);

            tokio::spawn(api::run_tree_poller(
                app.sender.clone(),
                cc.egui_ctx.clone(),
            ));

            Ok(Box::new(app))
        }),
    )
}

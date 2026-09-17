use crate::theme::Theme;
use egui::{CornerRadius, Frame, Margin, RichText, Stroke, Ui};
use zestors::{runtime::Pid, supervision::messages::Health};

pub fn render_health_card(ui: &mut Ui, pid: &Pid, health: &Health) {
    Frame::canvas(ui.style())
        .fill(Theme::INNER_CARD_BG)
        .stroke(Stroke::new(1.0, Theme::BORDER_COLOR))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.label(
                RichText::new("DEBUG STATE")
                    .small()
                    .strong()
                    .color(Theme::LABEL_MUTED),
            );
            ui.add_space(4.0);

            egui::Grid::new(ui.make_persistent_id(&format!("{}_debug_grid", pid)))
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    // // Actor Status Row with dynamic color badge
                    // ui.label(RichText::new("Status:").color(Theme::LABEL_MUTED));
                    // render_actor_status_badge(ui, &debug.status);
                    // ui.end_row();

                    ui.label(RichText::new("Uptime:").color(Theme::LABEL_MUTED));
                    // ui.label(RichText::new(format_duration(&"todo")).color(egui::Color32::WHITE));
                    ui.end_row();

                    ui.label(RichText::new("Description:").color(Theme::LABEL_MUTED));
                    ui.add(
                        egui::Label::new(
                            RichText::new(health.to_string()).color(egui::Color32::LIGHT_GRAY),
                        )
                        .wrap(),
                    );
                    ui.end_row();
                });
        });
}

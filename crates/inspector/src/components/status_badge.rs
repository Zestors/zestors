use crate::theme::Theme;
use egui::{CornerRadius, Frame, Margin, RichText, Ui};
use zestors::runtime::ActorStatus;

pub fn render_actor_status_badge(ui: &mut Ui, status: &ActorStatus) {
    let (label_text, badge_bg, badge_fg) = Theme::actor_status_style(status);

    Frame::canvas(ui.style())
        .fill(badge_bg)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(label_text).small().strong().color(badge_fg));
        });
}

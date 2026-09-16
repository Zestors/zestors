use egui::{Color32, Context, Stroke, Visuals};
use zestors::runtime::ActorStatus;

pub struct Theme;

impl Theme {
    pub const BG_DARK: Color32 = Color32::from_rgb(18, 20, 26);
    pub const CARD_BG: Color32 = Color32::from_rgb(26, 29, 38);
    pub const INNER_CARD_BG: Color32 = Color32::from_rgb(20, 22, 30);
    pub const BORDER_COLOR: Color32 = Color32::from_rgb(45, 50, 66);
    pub const PID_BLUE: Color32 = Color32::from_rgb(97, 175, 239);
    pub const LABEL_MUTED: Color32 = Color32::from_rgb(140, 148, 170);
    pub const VALUE_PURPLE: Color32 = Color32::from_rgb(198, 120, 221);
    pub const ERROR_RED: Color32 = Color32::from_rgb(224, 108, 117);

    pub fn apply(ctx: &Context) {
        let mut visuals = Visuals::dark();
        visuals.panel_fill = Self::BG_DARK;
        visuals.window_fill = Self::BG_DARK;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Self::BORDER_COLOR);
        ctx.set_visuals(visuals);
    }

    /// Resolves label text, background color, and foreground color for an `ActorStatus` variant.
    pub fn actor_status_style(status: &ActorStatus) -> (String, Color32, Color32) {
        match status {
            ActorStatus::Initializing => (
                "Initializing".to_string(),
                Color32::from_rgb(60, 50, 30),    // Amber BG
                Color32::from_rgb(229, 192, 123), // Amber FG
            ),
            ActorStatus::Running => (
                "Running".to_string(),
                Color32::from_rgb(34, 60, 45),    // Green BG
                Color32::from_rgb(152, 195, 121), // Green FG
            ),
            ActorStatus::Suspended => (
                "Suspended".to_string(),
                Color32::from_rgb(38, 45, 60),   // Blue BG
                Color32::from_rgb(97, 175, 239), // Blue FG
            ),
            ActorStatus::Exiting => (
                "Exiting".to_string(),
                Color32::from_rgb(70, 45, 25),    // Orange BG
                Color32::from_rgb(209, 154, 102), // Orange FG
            ),
            ActorStatus::Exited(exit) => (
                format!("Dead ({:?})", exit),
                Color32::from_rgb(65, 35, 40),    // Red BG
                Color32::from_rgb(224, 108, 117), // Red FG
            ),
        }
    }
}

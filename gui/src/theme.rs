use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Vec2,
};

use crosspost_core::domain::model::Platform;

pub const ACCENT: Color32 = Color32::from_rgb(88, 101, 242);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(110, 122, 255);
pub const ACCENT_ACTIVE: Color32 = Color32::from_rgb(70, 84, 220);
pub const SUCCESS: Color32 = Color32::from_rgb(72, 187, 120);
pub const WARNING: Color32 = Color32::from_rgb(237, 175, 55);
pub const DANGER: Color32 = Color32::from_rgb(235, 87, 87);
pub const MUTED: Color32 = Color32::from_rgb(140, 146, 163);

pub const CONTENT_MAX_WIDTH: f32 = 780.0;
pub const CARD_RADIUS: u8 = 12;
pub const WIDGET_RADIUS: u8 = 7;

pub fn platform_color(p: Platform) -> Color32 {
    match p {
        Platform::YouTube => Color32::from_rgb(229, 35, 35),
        Platform::TikTok => Color32::from_rgb(37, 244, 238),
        Platform::Instagram => Color32::from_rgb(225, 48, 108),
        Platform::VK => Color32::from_rgb(76, 117, 163),
    }
}

pub fn platform_badge(p: Platform) -> &'static str {
    match p {
        Platform::YouTube => "YT",
        Platform::TikTok => "TT",
        Platform::Instagram => "IG",
        Platform::VK => "VK",
    }
}

pub fn theme_icon(theme: crosspost_core::domain::model::ThemePreference) -> &'static str {
    use crosspost_core::domain::model::ThemePreference::*;
    match theme {
        Light => "\u{2600}",
        Dark => "\u{263E}",
        System => "\u{1F5A5}",
    }
}

pub fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    let widget_radius = CornerRadius::same(WIDGET_RADIUS);
    for widgets in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widgets.corner_radius = widget_radius;
    }
    style.visuals.window_corner_radius = CornerRadius::same(CARD_RADIUS);
    style.visuals.menu_corner_radius = CornerRadius::same(CARD_RADIUS);

    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_HOVER);
    style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    style.visuals.selection.stroke = Stroke::new(1.0, ACCENT_HOVER);
    style.visuals.hyperlink_color = ACCENT_HOVER;

    style.spacing.item_spacing = Vec2::new(8.0, 10.0);
    style.spacing.button_padding = Vec2::new(14.0, 8.0);
    style.spacing.interact_size = Vec2::new(26.0, 26.0);
    style.spacing.icon_width = 18.0;
    style.spacing.icon_spacing = 6.0;
    style.spacing.window_margin = Margin::same(14);

    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(22.0, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(12.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(13.0, FontFamily::Monospace),
    );

    ctx.set_style(style);
}

pub fn card_frame(ctx: &egui::Context) -> egui::Frame {
    let visuals = &ctx.style().visuals;
    let fill = if visuals.dark_mode {
        visuals.panel_fill.gamma_multiply(1.25)
    } else {
        Color32::from_rgb(250, 251, 253)
    };
    let stroke_color = if visuals.dark_mode {
        Color32::from_rgb(56, 60, 72)
    } else {
        Color32::from_rgb(220, 224, 232)
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke_color))
        .corner_radius(CornerRadius::same(CARD_RADIUS))
        .inner_margin(Margin::symmetric(16, 14))
        .outer_margin(Margin::ZERO)
}

pub fn section_title(ui: &mut egui::Ui, icon: &str, text: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon).size(15.0).color(ACCENT_HOVER));
        ui.label(
            egui::RichText::new(text)
                .size(15.0)
                .strong()
                .color(ui.visuals().strong_text_color()),
        );
    });
    ui.add_space(6.0);
}

pub fn status_dot(ui: &mut egui::Ui, color: Color32) {
    ui.label(egui::RichText::new("\u{25CF}").color(color).size(12.0));
}

pub fn platform_chip(ui: &mut egui::Ui, platform: Platform) {
    let color = platform_color(platform);
    let text_color = if color.intensity() > 0.6 {
        Color32::BLACK
    } else {
        Color32::WHITE
    };
    egui::Frame::new()
        .fill(color)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(platform_badge(platform))
                    .monospace()
                    .size(11.0)
                    .color(text_color)
                    .strong(),
            );
        });
}

pub fn primary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .size(15.0)
            .strong()
            .color(Color32::WHITE),
    )
    .fill(ACCENT)
    .stroke(Stroke::new(1.0, ACCENT_ACTIVE))
    .corner_radius(CornerRadius::same(WIDGET_RADIUS))
    .min_size(Vec2::new(220.0, 42.0))
}

pub fn secondary_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).size(13.0).strong().color(ACCENT_HOVER))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, ACCENT_HOVER))
        .corner_radius(CornerRadius::same(WIDGET_RADIUS))
}

trait ColorIntensity {
    fn intensity(&self) -> f32;
}

impl ColorIntensity for Color32 {
    fn intensity(&self) -> f32 {
        let [r, g, b, _] = self.to_array();
        (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0
    }
}

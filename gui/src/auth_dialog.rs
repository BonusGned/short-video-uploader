use eframe::egui;

use crosspost_core::domain::model::Platform;

use crate::theme;

#[derive(Default)]
pub struct AuthDialog {
    open_for: Option<Platform>,
    client_id: String,
    client_secret: String,
}

pub enum DialogAction {
    None,
    SaveAndLogin {
        platform: Platform,
        client_id: String,
        client_secret: String,
    },
}

impl AuthDialog {
    pub fn open(&mut self, platform: Platform, existing: (String, String)) {
        self.open_for = Some(platform);
        self.client_id = existing.0;
        self.client_secret = existing.1;
    }

    pub fn close(&mut self) {
        self.open_for = None;
        self.client_id.clear();
        self.client_secret.clear();
    }

    pub fn show(&mut self, ctx: &egui::Context) -> DialogAction {
        let Some(platform) = self.open_for else {
            return DialogAction::None;
        };

        let mut action = DialogAction::None;
        let mut should_close = false;

        egui::Window::new(format!("Configure {platform}"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(intro_text(platform))
                        .size(13.0)
                        .color(theme::MUTED),
                );

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Where do I get these?")
                            .size(12.0)
                            .color(theme::MUTED),
                    );
                    ui.hyperlink_to(help_label(platform), help_url(platform));
                });

                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("Client ID")
                        .size(13.0)
                        .color(theme::MUTED),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.client_id)
                        .hint_text("Paste your client ID\u{2026}")
                        .desired_width(f32::INFINITY),
                );

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Client Secret")
                        .size(13.0)
                        .color(theme::MUTED),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.client_secret)
                        .hint_text("Paste your client secret\u{2026}")
                        .password(true)
                        .desired_width(f32::INFINITY),
                );

                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(redirect_hint(platform))
                        .size(12.0)
                        .color(theme::MUTED),
                );

                ui.add_space(14.0);
                let can_save =
                    !self.client_id.trim().is_empty() && !self.client_secret.trim().is_empty();

                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        should_close = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(can_save, theme::primary_button("Save & Login"))
                            .clicked()
                        {
                            action = DialogAction::SaveAndLogin {
                                platform,
                                client_id: self.client_id.trim().to_string(),
                                client_secret: self.client_secret.trim().to_string(),
                            };
                            should_close = true;
                        }
                    });
                });
                ui.add_space(4.0);
            });

        if should_close {
            self.close();
        }

        action
    }
}

fn intro_text(platform: Platform) -> &'static str {
    match platform {
        Platform::YouTube => {
            "Create an OAuth 2.0 Client ID in Google Cloud Console (type: Desktop app) \
             and paste its credentials below. They will be saved locally."
        }
        Platform::VK => {
            "Create a standalone VK application and paste its Service/Client credentials below. \
             They will be saved locally."
        }
        _ => "Paste OAuth credentials for this platform below.",
    }
}

fn help_label(platform: Platform) -> &'static str {
    match platform {
        Platform::YouTube => "Google Cloud Console",
        Platform::VK => "VK for Developers",
        _ => "Docs",
    }
}

fn help_url(platform: Platform) -> &'static str {
    match platform {
        Platform::YouTube => "https://console.cloud.google.com/apis/credentials",
        Platform::VK => "https://dev.vk.com/en/admin/apps-list",
        _ => "https://example.com",
    }
}

fn redirect_hint(platform: Platform) -> String {
    let port = match platform {
        Platform::YouTube => 8585,
        Platform::VK => 8588,
        _ => 0,
    };
    format!("Redirect URI to register: http://localhost:{port}")
}

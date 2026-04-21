mod theme;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;

use crosspost_core::adapter::{self, oauth};
use crosspost_core::config::manager::ConfigManager;
use crosspost_core::domain::model::{
    Platform, ThemePreference, UploadResult, UploadStatus, VideoMetadata,
};
use crosspost_core::domain::port::{AsyncUploader, UploadProgress};
use crosspost_core::service::upload_orchestrator::UploadOrchestrator;
use crosspost_core::validation::VideoValidator;

#[derive(Default, Clone)]
struct UploadForm {
    title: String,
    description: String,
    tags_input: String,
    video_path: Option<PathBuf>,
    thumbnail_path: Option<PathBuf>,
    platforms: HashMap<Platform, bool>,
}

impl UploadForm {
    fn new() -> Self {
        let mut platforms = HashMap::new();
        for p in Platform::ALL {
            platforms.insert(p, true);
        }
        Self {
            platforms,
            ..Default::default()
        }
    }

    fn enabled_platforms(&self) -> Vec<Platform> {
        self.platforms
            .iter()
            .filter(|(_, enabled)| **enabled)
            .map(|(&p, _)| p)
            .collect()
    }

    fn to_metadata(&self) -> Option<VideoMetadata> {
        let path = self.video_path.as_ref()?;
        let mut meta = VideoMetadata::new(self.title.clone(), path.clone());
        if !self.description.is_empty() {
            meta = meta.with_description(self.description.clone());
        }
        let tags: Vec<String> = self
            .tags_input
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !tags.is_empty() {
            meta = meta.with_tags(tags);
        }
        if let Some(ref thumb) = self.thumbnail_path {
            meta = meta.with_thumbnail(thumb.clone());
        }
        Some(meta)
    }
}

#[derive(Clone)]
struct PlatformProgress {
    bytes_sent: u64,
    total_bytes: u64,
}

enum AppState {
    Idle,
    Uploading,
    Done(Vec<UploadResult>),
}

struct CrossPostApp {
    config_manager: ConfigManager,
    form: UploadForm,
    state: AppState,
    progress: Arc<Mutex<HashMap<Platform, PlatformProgress>>>,
    validation_errors: Vec<String>,
    runtime: tokio::runtime::Runtime,
    auth_status: HashMap<Platform, AuthState>,
}

#[derive(Clone, PartialEq)]
enum AuthState {
    NotConfigured,
    NotAuthenticated,
    Authenticated,
    Expired,
}

impl CrossPostApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config_manager = ConfigManager::new().expect("Failed to initialize config");
        apply_theme(&cc.egui_ctx, config_manager.config().theme);
        theme::setup_style(&cc.egui_ctx);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        let auth_status = Self::check_auth_status(&config_manager);

        Self {
            config_manager,
            form: UploadForm::new(),
            state: AppState::Idle,
            progress: Arc::new(Mutex::new(HashMap::new())),
            validation_errors: Vec::new(),
            runtime,
            auth_status,
        }
    }

    fn check_auth_status(config_manager: &ConfigManager) -> HashMap<Platform, AuthState> {
        let config = config_manager.config();
        let mut status = HashMap::new();

        for platform in Platform::ALL {
            let configured = match platform {
                Platform::YouTube => config.youtube.is_configured(),
                Platform::TikTok => config.tiktok.is_configured(),
                Platform::Instagram => config.instagram.is_configured(),
                Platform::VK => config.vk.is_configured(),
            };

            if !configured {
                status.insert(platform, AuthState::NotConfigured);
                continue;
            }

            let state = match oauth::load_token(platform) {
                Ok(Some(t)) if !t.is_expired() => AuthState::Authenticated,
                Ok(Some(_)) => AuthState::Expired,
                _ => AuthState::NotAuthenticated,
            };
            status.insert(platform, state);
        }

        status
    }

    fn refresh_auth_status(&mut self) {
        self.auth_status = Self::check_auth_status(&self.config_manager);
    }

    fn render_top_bar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("CrossPost")
                        .size(24.0)
                        .strong()
                        .color(ui.visuals().strong_text_color()),
                );
                ui.label(
                    egui::RichText::new("Publish once, share everywhere")
                        .size(12.0)
                        .color(theme::MUTED),
                );
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let current = self.config_manager.config().theme;
                let icon = theme::theme_icon(current);
                let label = match current {
                    ThemePreference::Light => "Light",
                    ThemePreference::Dark => "Dark",
                    ThemePreference::System => "System",
                };
                let btn =
                    egui::Button::new(egui::RichText::new(format!("{icon}  {label}")).size(13.0))
                        .min_size(egui::vec2(96.0, 30.0));
                if ui.add(btn).clicked() {
                    let next = match current {
                        ThemePreference::Light => ThemePreference::Dark,
                        ThemePreference::Dark => ThemePreference::System,
                        ThemePreference::System => ThemePreference::Light,
                    };
                    let _ = self.config_manager.update(|c| c.theme = next);
                    apply_theme(ctx, next);
                    theme::setup_style(ctx);
                }
            });
        });
        ui.add_space(6.0);
    }

    fn render_auth_section(&mut self, ui: &mut egui::Ui) {
        theme::card_frame(ui.ctx()).show(ui, |ui| {
            theme::section_title(ui, "\u{1F511}", "Authentication");

            for (i, platform) in Platform::ALL.iter().enumerate() {
                if i > 0 {
                    ui.add_space(2.0);
                }
                let platform = *platform;
                let state = self
                    .auth_status
                    .get(&platform)
                    .cloned()
                    .unwrap_or(AuthState::NotConfigured);

                ui.horizontal(|ui| {
                    theme::platform_chip(ui, platform);
                    ui.label(
                        egui::RichText::new(platform.to_string())
                            .color(ui.visuals().text_color()),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        match state {
                            AuthState::Authenticated => {
                                if ui.small_button("Logout").clicked() {
                                    let _ = crosspost_core::adapter::keyring_store::KeyringStore::delete_token(platform);
                                    self.refresh_auth_status();
                                }
                            }
                            AuthState::NotAuthenticated | AuthState::Expired => {
                                if ui.small_button("Login").clicked() {
                                    self.start_auth(platform, ui.ctx());
                                }
                            }
                            AuthState::NotConfigured => {}
                        }

                        let (color, label) = match state {
                            AuthState::Authenticated => (theme::SUCCESS, "Authenticated"),
                            AuthState::Expired => (theme::WARNING, "Expired (auto-refresh)"),
                            AuthState::NotAuthenticated => (theme::DANGER, "Not authenticated"),
                            AuthState::NotConfigured => (theme::MUTED, "Not configured"),
                        };
                        ui.label(egui::RichText::new(label).color(color).size(13.0));
                        theme::status_dot(ui, color);
                    });
                });
            }
        });
    }

    fn render_file_section(&mut self, ui: &mut egui::Ui) {
        theme::card_frame(ui.ctx()).show(ui, |ui| {
            theme::section_title(ui, "\u{1F4C1}", "Source Files");

            ui.label(egui::RichText::new("Video").size(13.0).color(theme::MUTED));
            ui.horizontal(|ui| {
                let display = self
                    .form
                    .video_path
                    .as_ref()
                    .map(|p| truncate_path(&p.display().to_string(), 60))
                    .unwrap_or_else(|| "No file selected".into());
                let has_file = self.form.video_path.is_some();
                ui.label(egui::RichText::new(display).color(if has_file {
                    ui.visuals().text_color()
                } else {
                    theme::MUTED
                }));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Browse\u{2026}").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Video", &["mp4", "mov", "webm", "avi"])
                            .pick_file()
                    {
                        self.form.video_path = Some(path);
                        self.validation_errors.clear();
                    }
                    if has_file && ui.small_button("Clear").clicked() {
                        self.form.video_path = None;
                    }
                });
            });

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Thumbnail (optional)")
                    .size(13.0)
                    .color(theme::MUTED),
            );
            ui.horizontal(|ui| {
                let display = self
                    .form
                    .thumbnail_path
                    .as_ref()
                    .map(|p| truncate_path(&p.display().to_string(), 60))
                    .unwrap_or_else(|| "No thumbnail".into());
                let has_thumb = self.form.thumbnail_path.is_some();
                ui.label(egui::RichText::new(display).color(if has_thumb {
                    ui.visuals().text_color()
                } else {
                    theme::MUTED
                }));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Browse\u{2026}").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("Image", &["png", "jpg", "jpeg", "webp"])
                            .pick_file()
                    {
                        self.form.thumbnail_path = Some(path);
                    }
                    if has_thumb && ui.small_button("Clear").clicked() {
                        self.form.thumbnail_path = None;
                    }
                });
            });
        });
    }

    fn render_metadata_section(&mut self, ui: &mut egui::Ui) {
        theme::card_frame(ui.ctx()).show(ui, |ui| {
            theme::section_title(ui, "\u{270E}", "Metadata");

            ui.label(egui::RichText::new("Title").size(13.0).color(theme::MUTED));
            ui.add(
                egui::TextEdit::singleline(&mut self.form.title)
                    .hint_text("Give your video a title\u{2026}")
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Description")
                    .size(13.0)
                    .color(theme::MUTED),
            );
            ui.add(
                egui::TextEdit::multiline(&mut self.form.description)
                    .hint_text("Tell viewers about your video\u{2026}")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );

            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Tags (comma-separated)")
                    .size(13.0)
                    .color(theme::MUTED),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.form.tags_input)
                    .hint_text("tutorial, rust, demo")
                    .desired_width(f32::INFINITY),
            );
        });
    }

    fn render_platform_section(&mut self, ui: &mut egui::Ui) {
        theme::card_frame(ui.ctx()).show(ui, |ui| {
            theme::section_title(ui, "\u{1F4E1}", "Publish to");
            ui.horizontal_wrapped(|ui| {
                for platform in Platform::ALL {
                    let enabled = self.form.platforms.entry(platform).or_insert(true);
                    ui.horizontal(|ui| {
                        ui.checkbox(enabled, "");
                        theme::platform_chip(ui, platform);
                        ui.label(
                            egui::RichText::new(platform.to_string())
                                .color(ui.visuals().text_color()),
                        );
                    });
                    ui.add_space(6.0);
                }
            });
        });
    }

    fn render_progress(&self, ui: &mut egui::Ui) {
        let progress = self.progress.lock().unwrap();
        if progress.is_empty() {
            return;
        }

        theme::card_frame(ui.ctx()).show(ui, |ui| {
            theme::section_title(ui, "\u{21E7}", "Upload progress");
            for platform in Platform::ALL {
                if let Some(p) = progress.get(&platform) {
                    let fraction = if p.total_bytes > 0 {
                        p.bytes_sent as f32 / p.total_bytes as f32
                    } else {
                        0.0
                    };
                    ui.horizontal(|ui| {
                        theme::platform_chip(ui, platform);
                        ui.label(platform.to_string());
                    });
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .text(format!(
                                "{:.0}%  \u{2022}  {} / {}",
                                fraction * 100.0,
                                humanize_bytes(p.bytes_sent),
                                humanize_bytes(p.total_bytes)
                            ))
                            .desired_width(f32::INFINITY)
                            .animate(true),
                    );
                    ui.add_space(4.0);
                }
            }
        });
    }

    fn render_results(&self, ui: &mut egui::Ui, results: &[UploadResult]) {
        theme::card_frame(ui.ctx()).show(ui, |ui| {
            theme::section_title(ui, "\u{2728}", "Results");
            for r in results {
                ui.horizontal(|ui| {
                    theme::platform_chip(ui, r.platform);
                    match &r.status {
                        UploadStatus::Success { url } => {
                            ui.label(egui::RichText::new("\u{2713}").color(theme::SUCCESS));
                            ui.hyperlink_to(
                                egui::RichText::new(truncate_path(url, 70))
                                    .color(theme::ACCENT_HOVER),
                                url,
                            );
                        }
                        UploadStatus::Failed { reason } => {
                            ui.label(egui::RichText::new("\u{2717}").color(theme::DANGER));
                            ui.label(egui::RichText::new(reason).color(theme::DANGER));
                        }
                    }
                });
            }
        });
    }

    fn render_validation_errors(&self, ui: &mut egui::Ui) {
        if self.validation_errors.is_empty() {
            return;
        }
        theme::card_frame(ui.ctx()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("\u{26A0}")
                        .color(theme::DANGER)
                        .size(15.0),
                );
                ui.label(
                    egui::RichText::new("Please fix the following")
                        .strong()
                        .color(theme::DANGER),
                );
            });
            ui.add_space(4.0);
            for err in &self.validation_errors {
                ui.label(egui::RichText::new(format!("\u{2022}  {err}")).color(theme::DANGER));
            }
        });
    }

    fn start_auth(&mut self, platform: Platform, ctx: &egui::Context) {
        let uploaders = adapter::create_uploaders(self.config_manager.config());
        let uploader = uploaders.into_iter().find(|u| u.platform() == platform);

        let Some(uploader) = uploader else { return };

        let ctx = ctx.clone();
        self.runtime.spawn(async move {
            let result = uploader.authenticate().await;
            AUTH_RESULT
                .lock()
                .unwrap()
                .replace((platform, result.is_ok()));
            ctx.request_repaint();
        });
    }

    fn start_upload(&mut self, ctx: &egui::Context) {
        let Some(metadata) = self.form.to_metadata() else {
            self.validation_errors = vec!["Please select a video file.".into()];
            return;
        };

        if self.form.title.trim().is_empty() {
            self.validation_errors = vec!["Title is required.".into()];
            return;
        }

        let platforms = self.form.enabled_platforms();
        if platforms.is_empty() {
            self.validation_errors = vec!["Select at least one platform.".into()];
            return;
        }

        let reports = VideoValidator::validate(&metadata, &platforms);
        let errors: Vec<String> = reports
            .iter()
            .flat_map(|r| r.errors.iter().cloned())
            .collect();

        if !errors.is_empty() {
            self.validation_errors = errors;
            return;
        }

        self.validation_errors.clear();
        self.state = AppState::Uploading;
        self.progress.lock().unwrap().clear();

        let all_uploaders = adapter::create_uploaders(self.config_manager.config());
        let uploaders: Vec<Arc<dyn AsyncUploader>> = all_uploaders
            .into_iter()
            .filter(|u| platforms.contains(&u.platform()))
            .collect();

        let orchestrator = UploadOrchestrator::new(uploaders);
        let progress = Arc::clone(&self.progress);
        let ctx = ctx.clone();

        self.runtime.spawn(async move {
            let _ = orchestrator.authenticate_all().await;

            let progress_cb = {
                let progress = Arc::clone(&progress);
                let ctx = ctx.clone();
                move |platform: Platform, up: UploadProgress| {
                    progress.lock().unwrap().insert(
                        platform,
                        PlatformProgress {
                            bytes_sent: up.bytes_sent,
                            total_bytes: up.total_bytes,
                        },
                    );
                    ctx.request_repaint();
                }
            };

            let results = orchestrator.upload_all(&metadata, progress_cb).await;

            UPLOAD_RESULTS
                .lock()
                .unwrap()
                .replace(results.unwrap_or_default());
            ctx.request_repaint();
        });
    }

    fn render_idle_body(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.render_auth_section(ui);
        ui.add_space(12.0);
        self.render_file_section(ui);
        ui.add_space(12.0);
        self.render_metadata_section(ui);
        ui.add_space(12.0);
        self.render_platform_section(ui);

        if !self.validation_errors.is_empty() {
            ui.add_space(12.0);
            self.render_validation_errors(ui);
        }

        ui.add_space(18.0);
        let selected_count = self.form.enabled_platforms().len();
        let can_upload = self.form.video_path.is_some()
            && !self.form.title.trim().is_empty()
            && selected_count > 0;

        ui.vertical_centered(|ui| {
            let label = if selected_count == 0 {
                "Upload".to_string()
            } else {
                format!(
                    "\u{21E7}  Upload to {selected_count} platform{}",
                    if selected_count == 1 { "" } else { "s" }
                )
            };
            if ui
                .add_enabled(can_upload, theme::primary_button(&label))
                .clicked()
            {
                self.start_upload(ctx);
            }
        });
        ui.add_space(4.0);
    }

    fn render_uploading_body(&self, ui: &mut egui::Ui) {
        self.render_progress(ui);
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.spinner();
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Uploading to selected platforms\u{2026}").color(theme::MUTED),
            );
        });
    }

    fn render_done_body(&mut self, ui: &mut egui::Ui, results: &[UploadResult]) {
        self.render_results(ui, results);
        ui.add_space(18.0);
        ui.vertical_centered(|ui| {
            if ui
                .add(theme::primary_button("\u{21BB}  New upload"))
                .clicked()
            {
                self.state = AppState::Idle;
                self.progress.lock().unwrap().clear();
            }
        });
    }
}

static UPLOAD_RESULTS: Mutex<Option<Vec<UploadResult>>> = Mutex::new(None);
static AUTH_RESULT: Mutex<Option<(Platform, bool)>> = Mutex::new(None);

impl eframe::App for CrossPostApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(results) = UPLOAD_RESULTS.lock().unwrap().take() {
            self.state = AppState::Done(results);
        }

        if AUTH_RESULT.lock().unwrap().take().is_some() {
            self.refresh_auth_status();
        }

        let top_frame = egui::Frame::new()
            .fill(ctx.style().visuals.panel_fill)
            .inner_margin(egui::Margin::symmetric(20, 8))
            .stroke(egui::Stroke::new(
                1.0,
                ctx.style().visuals.widgets.noninteractive.bg_stroke.color,
            ));

        egui::TopBottomPanel::top("top_panel")
            .frame(top_frame)
            .show(ctx, |ui| {
                self.render_top_bar(ctx, ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.set_max_width(theme::CONTENT_MAX_WIDTH);
                    ui.add_space(12.0);

                    match &self.state {
                        AppState::Idle => self.render_idle_body(ui, ctx),
                        AppState::Uploading => self.render_uploading_body(ui),
                        AppState::Done(results) => {
                            let cloned = results.clone();
                            self.render_done_body(ui, &cloned);
                        }
                    }

                    ui.add_space(16.0);
                });
            });
        });

        if matches!(self.state, AppState::Uploading) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

fn apply_theme(ctx: &egui::Context, pref: ThemePreference) {
    match pref {
        ThemePreference::Light => ctx.set_theme(egui::Theme::Light),
        ThemePreference::Dark => ctx.set_theme(egui::Theme::Dark),
        ThemePreference::System => ctx.set_theme(egui::ThemePreference::System),
    }
}

fn truncate_path(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let tail: String = s.chars().rev().take(max.saturating_sub(3)).collect();
    let tail: String = tail.chars().rev().collect();
    format!("\u{2026}{tail}")
}

fn humanize_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn main() -> eframe::Result {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([880.0, 720.0])
            .with_min_inner_size([640.0, 520.0])
            .with_title("CrossPost"),
        ..Default::default()
    };

    eframe::run_native(
        "CrossPost",
        options,
        Box::new(|cc| Ok(Box::new(CrossPostApp::new(cc)))),
    )
}

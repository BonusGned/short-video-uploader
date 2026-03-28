use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;

use crosspost_core::adapter::mock_uploader::MockUploader;
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
}

impl CrossPostApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config_manager = ConfigManager::new().expect("Failed to initialize config");
        apply_theme(&cc.egui_ctx, config_manager.config().theme);

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        Self {
            config_manager,
            form: UploadForm::new(),
            state: AppState::Idle,
            progress: Arc::new(Mutex::new(HashMap::new())),
            validation_errors: Vec::new(),
            runtime,
        }
    }

    fn render_top_bar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("CrossPost");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let current = self.config_manager.config().theme;
                let label = match current {
                    ThemePreference::Light => "Light",
                    ThemePreference::Dark => "Dark",
                    ThemePreference::System => "System",
                };

                if ui.button(label).clicked() {
                    let next = match current {
                        ThemePreference::Light => ThemePreference::Dark,
                        ThemePreference::Dark => ThemePreference::System,
                        ThemePreference::System => ThemePreference::Light,
                    };
                    let _ = self.config_manager.update(|c| c.theme = next);
                    apply_theme(ctx, next);
                }
                ui.label("Theme:");
            });
        });
    }

    fn render_file_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Video File").strong());
            ui.horizontal(|ui| {
                let label = self
                    .form
                    .video_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "No file selected".into());
                ui.label(&label);
                if ui.button("Browse...").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Video", &["mp4", "mov", "webm", "avi"])
                        .pick_file()
                {
                    self.form.video_path = Some(path);
                    self.validation_errors.clear();
                }
            });

            ui.add_space(4.0);
            ui.label(egui::RichText::new("Thumbnail (optional)").strong());
            ui.horizontal(|ui| {
                let label = self
                    .form
                    .thumbnail_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "No thumbnail".into());
                ui.label(&label);
                if ui.button("Browse...").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Image", &["png", "jpg", "jpeg", "webp"])
                        .pick_file()
                {
                    self.form.thumbnail_path = Some(path);
                }
                if self.form.thumbnail_path.is_some() && ui.button("Clear").clicked() {
                    self.form.thumbnail_path = None;
                }
            });
        });
    }

    fn render_metadata_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Metadata").strong());

            ui.horizontal(|ui| {
                ui.label("Title:");
                ui.text_edit_singleline(&mut self.form.title);
            });

            ui.horizontal(|ui| {
                ui.label("Description:");
            });
            ui.add(
                egui::TextEdit::multiline(&mut self.form.description)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );

            ui.horizontal(|ui| {
                ui.label("Tags (comma-separated):");
                ui.text_edit_singleline(&mut self.form.tags_input);
            });
        });
    }

    fn render_platform_section(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Platforms").strong());
            ui.horizontal(|ui| {
                for platform in Platform::ALL {
                    let enabled = self.form.platforms.entry(platform).or_insert(true);
                    ui.checkbox(enabled, platform.to_string());
                }
            });
        });
    }

    fn render_progress(&self, ui: &mut egui::Ui) {
        let progress = self.progress.lock().unwrap();
        if progress.is_empty() {
            return;
        }

        ui.group(|ui| {
            ui.label(egui::RichText::new("Upload Progress").strong());
            for platform in Platform::ALL {
                if let Some(p) = progress.get(&platform) {
                    let fraction = if p.total_bytes > 0 {
                        p.bytes_sent as f32 / p.total_bytes as f32
                    } else {
                        0.0
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!("{platform}:"));
                        ui.add(
                            egui::ProgressBar::new(fraction)
                                .text(format!("{:.0}%", fraction * 100.0))
                                .animate(true),
                        );
                    });
                }
            }
        });
    }

    fn render_results(&self, ui: &mut egui::Ui, results: &[UploadResult]) {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Results").strong());
            for r in results {
                match &r.status {
                    UploadStatus::Success { url } => {
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 200, 80),
                            format!("[{}] {url}", r.platform),
                        );
                    }
                    UploadStatus::Failed { reason } => {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 80, 80),
                            format!("[{}] {reason}", r.platform),
                        );
                    }
                }
            }
            if ui.button("New Upload").clicked() {
                // handled in update()
            }
        });
    }

    fn render_validation_errors(&self, ui: &mut egui::Ui) {
        if self.validation_errors.is_empty() {
            return;
        }
        ui.group(|ui| {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 80), "Validation Errors:");
            for err in &self.validation_errors {
                ui.label(format!("  - {err}"));
            }
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

        let uploaders: Vec<Arc<dyn AsyncUploader>> = platforms
            .iter()
            .map(|&p| Arc::new(MockUploader::new(p).with_delay(3000)) as Arc<dyn AsyncUploader>)
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
}

static UPLOAD_RESULTS: Mutex<Option<Vec<UploadResult>>> = Mutex::new(None);

impl eframe::App for CrossPostApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(results) = UPLOAD_RESULTS.lock().unwrap().take() {
            self.state = AppState::Done(results);
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            self.render_top_bar(ctx, ui);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                match &self.state {
                    AppState::Idle => {
                        self.render_file_section(ui);
                        ui.add_space(8.0);
                        self.render_metadata_section(ui);
                        ui.add_space(8.0);
                        self.render_platform_section(ui);
                        ui.add_space(8.0);
                        self.render_validation_errors(ui);
                        ui.add_space(8.0);

                        let can_upload = self.form.video_path.is_some()
                            && !self.form.title.trim().is_empty();

                        if ui
                            .add_enabled(can_upload, egui::Button::new("Upload"))
                            .clicked()
                        {
                            self.start_upload(ctx);
                        }
                    }
                    AppState::Uploading => {
                        self.render_progress(ui);
                        ui.add_space(8.0);
                        ui.spinner();
                        ui.label("Uploading to selected platforms...");
                    }
                    AppState::Done(results) => {
                        let results_clone = results.clone();
                        self.render_results(ui, &results_clone);
                        if ui.button("New Upload").clicked() {
                            self.state = AppState::Idle;
                            self.progress.lock().unwrap().clear();
                        }
                    }
                }
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

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "CrossPost",
        options,
        Box::new(|cc| Ok(Box::new(CrossPostApp::new(cc)))),
    )
}

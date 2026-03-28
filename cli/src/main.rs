use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crosspost_core::adapter::mock_uploader::MockUploader;
use crosspost_core::config::manager::ConfigManager;
use crosspost_core::domain::model::{Platform, ThemePreference, UploadStatus, VideoMetadata};
use crosspost_core::domain::port::AsyncUploader;
use crosspost_core::service::upload_orchestrator::UploadOrchestrator;
use crosspost_core::validation::VideoValidator;

#[derive(Parser)]
#[command(name = "crosspost", version, about = "Multi-platform shorts uploader")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Upload {
        #[arg(short, long)]
        video: String,

        #[arg(short, long)]
        title: String,

        #[arg(short, long)]
        description: Option<String>,

        #[arg(long, value_delimiter = ',')]
        tags: Option<Vec<String>>,

        #[arg(long)]
        thumbnail: Option<String>,

        #[arg(short, long, value_delimiter = ',', default_value = "youtube,instagram,tiktok,vk")]
        platforms: Vec<PlatformArg>,
    },
    Validate {
        #[arg(short, long)]
        video: String,

        #[arg(short, long, value_delimiter = ',', default_value = "youtube,instagram,tiktok,vk")]
        platforms: Vec<PlatformArg>,
    },
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    Show,
    SetTheme {
        #[arg(value_enum)]
        theme: ThemeArg,
    },
}

#[derive(ValueEnum, Clone)]
enum ThemeArg {
    Light,
    Dark,
    System,
}

#[derive(ValueEnum, Clone, Copy)]
enum PlatformArg {
    Youtube,
    Instagram,
    Tiktok,
    Vk,
}

impl From<PlatformArg> for Platform {
    fn from(p: PlatformArg) -> Self {
        match p {
            PlatformArg::Youtube => Platform::YouTube,
            PlatformArg::Instagram => Platform::Instagram,
            PlatformArg::Tiktok => Platform::TikTok,
            PlatformArg::Vk => Platform::VK,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Upload {
            video,
            title,
            description,
            tags,
            thumbnail,
            platforms,
        } => {
            let metadata = build_metadata(video, title, description, tags, thumbnail);
            let target_platforms: Vec<Platform> = platforms.into_iter().map(Into::into).collect();

            run_upload(metadata, target_platforms).await?;
        }
        Commands::Validate { video, platforms } => {
            let metadata = VideoMetadata::new("validation-check", video.into());
            let target: Vec<Platform> = platforms.into_iter().map(Into::into).collect();

            let reports = VideoValidator::validate(&metadata, &target);
            for report in &reports {
                if report.is_valid() {
                    println!("[{}] PASS", report.platform);
                } else {
                    println!("[{}] FAIL:", report.platform);
                    for err in &report.errors {
                        println!("  - {err}");
                    }
                }
            }
        }
        Commands::Config { action } => {
            handle_config(action)?;
        }
    }

    Ok(())
}

fn build_metadata(
    video: String,
    title: String,
    description: Option<String>,
    tags: Option<Vec<String>>,
    thumbnail: Option<String>,
) -> VideoMetadata {
    let mut meta = VideoMetadata::new(title, video.into());
    if let Some(d) = description {
        meta = meta.with_description(d);
    }
    if let Some(t) = tags {
        meta = meta.with_tags(t);
    }
    if let Some(th) = thumbnail {
        meta = meta.with_thumbnail(th.into());
    }
    meta
}

async fn run_upload(metadata: VideoMetadata, platforms: Vec<Platform>) -> Result<()> {
    let uploaders: Vec<Arc<dyn AsyncUploader>> = platforms
        .iter()
        .map(|&p| Arc::new(MockUploader::new(p).with_delay(2000)) as Arc<dyn AsyncUploader>)
        .collect();

    let orchestrator = UploadOrchestrator::new(uploaders);

    println!("Authenticating...");
    let auth_results = orchestrator.authenticate_all().await;
    for (platform, result) in &auth_results {
        match result {
            Ok(()) => println!("  [{platform}] Authenticated"),
            Err(e) => println!("  [{platform}] Auth failed: {e}"),
        }
    }

    let mp = MultiProgress::new();
    let style = ProgressStyle::with_template(
        "{prefix:>12.cyan.bold} [{bar:30.green/dim}] {bytes}/{total_bytes} {msg}",
    )?
    .progress_chars("=> ");

    let bars: Arc<Mutex<HashMap<Platform, ProgressBar>>> = Arc::new(Mutex::new(HashMap::new()));

    for &platform in &platforms {
        let pb = mp.add(ProgressBar::new(0));
        pb.set_style(style.clone());
        pb.set_prefix(platform.to_string());
        pb.set_message("waiting...");
        bars.lock().unwrap().insert(platform, pb);
    }

    let bars_cb = Arc::clone(&bars);
    let results = orchestrator
        .upload_all(&metadata, move |platform, progress| {
            if let Some(pb) = bars_cb.lock().unwrap().get(&platform) {
                pb.set_length(progress.total_bytes);
                pb.set_position(progress.bytes_sent);
                pb.set_message("uploading...");
            }
        })
        .await?;

    for (_, pb) in bars.lock().unwrap().iter() {
        pb.finish();
    }

    println!("\nResults:");
    for result in &results {
        match &result.status {
            UploadStatus::Success { url } => {
                println!("  [{}] Success: {url}", result.platform);
            }
            UploadStatus::Failed { reason } => {
                println!("  [{}] Failed: {reason}", result.platform);
            }
        }
    }

    Ok(())
}

fn handle_config(action: ConfigAction) -> Result<()> {
    let mut config_manager = ConfigManager::new()?;
    match action {
        ConfigAction::Show => {
            let config = config_manager.config();
            println!("Theme: {}", config.theme);
            println!("Default title: {:?}", config.default_title);
            println!("Enabled platforms: {:?}", config.enabled_platforms);
            println!("Config path: {}", config_manager.config_file_path().display());
        }
        ConfigAction::SetTheme { theme } => {
            let pref = match theme {
                ThemeArg::Light => ThemePreference::Light,
                ThemeArg::Dark => ThemePreference::Dark,
                ThemeArg::System => ThemePreference::System,
            };
            config_manager.update(|c| c.theme = pref)?;
            println!("Theme set to: {pref}");
        }
    }
    Ok(())
}

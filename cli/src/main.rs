use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crosspost_core::adapter::{self, oauth};
use crosspost_core::config::manager::ConfigManager;
use crosspost_core::domain::model::{Platform, ThemePreference, UploadStatus, VideoMetadata};
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

        #[arg(short, long, value_delimiter = ',')]
        platforms: Option<Vec<PlatformArg>>,
    },
    Validate {
        #[arg(short, long)]
        video: String,

        #[arg(short, long, value_delimiter = ',', default_value = "youtube,instagram,tiktok,vk")]
        platforms: Vec<PlatformArg>,
    },
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum AuthAction {
    Login {
        #[arg(short, long)]
        platform: PlatformArg,
    },
    Status,
    Logout {
        #[arg(short, long)]
        platform: PlatformArg,
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
    env_logger::init();
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
            let config_manager = ConfigManager::new()?;
            let metadata = build_metadata(video, title, description, tags, thumbnail);

            let uploaders = if let Some(plats) = platforms {
                let target: Vec<Platform> = plats.into_iter().map(Into::into).collect();
                let all = adapter::create_uploaders(config_manager.config());
                all.into_iter()
                    .filter(|u| target.contains(&u.platform()))
                    .collect()
            } else {
                adapter::create_uploaders(config_manager.config())
            };

            if uploaders.is_empty() {
                anyhow::bail!("No uploaders available. Configure credentials first.");
            }

            run_upload(metadata, uploaders).await?;
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
        Commands::Auth { action } => {
            handle_auth(action).await?;
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

async fn run_upload(
    metadata: VideoMetadata,
    uploaders: Vec<Arc<dyn crosspost_core::domain::port::AsyncUploader>>,
) -> Result<()> {
    let platforms: Vec<Platform> = uploaders.iter().map(|u| u.platform()).collect();
    let orchestrator = UploadOrchestrator::new(uploaders);

    println!("Authenticating...");
    let auth_results = orchestrator.authenticate_all().await;
    for (platform, result) in &auth_results {
        match result {
            Ok(()) => println!("  [{platform}] OK"),
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

async fn handle_auth(action: AuthAction) -> Result<()> {
    match action {
        AuthAction::Login { platform } => {
            let config_manager = ConfigManager::new()?;
            let plat: Platform = platform.into();
            let uploaders = adapter::create_uploaders(config_manager.config());
            let uploader = uploaders
                .iter()
                .find(|u| u.platform() == plat)
                .ok_or_else(|| anyhow::anyhow!("{plat} not configured. Add credentials to config first."))?;

            println!("Opening browser for {plat} authorization...");
            uploader.authenticate().await?;
            println!("{plat} authenticated successfully!");
        }
        AuthAction::Status => {
            for platform in Platform::ALL {
                let token = oauth::load_token(platform);
                let status = match token {
                    Ok(Some(t)) if !t.is_expired() => "authenticated",
                    Ok(Some(_)) => "expired (will auto-refresh)",
                    Ok(None) => "not authenticated",
                    Err(_) => "error reading token",
                };
                println!("  [{platform}] {status}");
            }
        }
        AuthAction::Logout { platform } => {
            let plat: Platform = platform.into();
            crosspost_core::adapter::keyring_store::KeyringStore::delete_token(plat)?;
            println!("{plat} token removed.");
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
            println!("Enabled platforms: {:?}", config.enabled_platforms);
            println!();
            println!("YouTube: {}", if config.youtube.is_configured() { "configured" } else { "not configured" });
            println!("TikTok: {}", if config.tiktok.is_configured() { "configured" } else { "not configured" });
            println!("Instagram: {}", if config.instagram.is_configured() { "configured" } else { "not configured" });
            println!("VK: {}", if config.vk.is_configured() { "configured" } else { "not configured" });
            println!();
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

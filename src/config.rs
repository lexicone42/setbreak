use std::path::PathBuf;

use directories::ProjectDirs;
use serde::Deserialize;

use crate::bands::CustomBandConfig;

/// Application configuration loaded from TOML config file.
/// All fields have sensible defaults — the config file is optional.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct AppConfig {
    /// Directories to scan for music files (used when `scan` has no CLI args).
    pub music_dirs: Vec<PathBuf>,
    /// Custom database path (overrides XDG default).
    pub db_path: Option<PathBuf>,
    /// Number of parallel workers. 0 = auto-detect (cores / 2, min 1).
    pub workers: usize,
    /// Archive.org API settings.
    pub archive: ArchiveConfig,
    /// Custom band definitions (merged with built-in registry).
    #[serde(rename = "bands")]
    pub custom_bands: Vec<CustomBandConfig>,
}

/// Archive.org API configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ArchiveConfig {
    /// Cache TTL in days before re-fetching from archive.org.
    pub cache_ttl_days: i64,
    /// Rate limit between API requests in milliseconds.
    pub rate_limit_ms: u64,
}

impl Default for ArchiveConfig {
    fn default() -> Self {
        Self {
            cache_ttl_days: 30,
            rate_limit_ms: 500,
        }
    }
}

impl AppConfig {
    /// Load config from `~/.config/setbreak/config.toml`.
    /// Returns default config if file doesn't exist.
    /// Logs a warning if the file exists but can't be parsed.
    pub fn load() -> Self {
        let config_path = Self::config_path();
        match config_path {
            Some(path) if path.exists() => {
                match std::fs::read_to_string(&path) {
                    Ok(contents) => {
                        match toml::from_str::<AppConfig>(&contents) {
                            Ok(config) => {
                                log::info!("Loaded config from {}", path.display());
                                config
                            }
                            Err(e) => {
                                log::warn!(
                                    "Failed to parse {}: {}. Using defaults.",
                                    path.display(),
                                    e
                                );
                                Self::default()
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to read {}: {}. Using defaults.",
                            path.display(),
                            e
                        );
                        Self::default()
                    }
                }
            }
            _ => {
                log::debug!("No config file found, using defaults");
                Self::default()
            }
        }
    }

    /// Resolve worker count: 0 → auto-detect (cores / 2, min 1).
    pub fn resolve_workers(&self) -> usize {
        if self.workers > 0 {
            self.workers
        } else {
            let cores = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(2);
            (cores / 2).max(1)
        }
    }

    /// Get the config file path.
    fn config_path() -> Option<PathBuf> {
        ProjectDirs::from("", "", crate::APP_NAME)
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }
}

/// Resolve the default database path using XDG data directory.
pub fn default_db_path() -> PathBuf {
    if let Some(dirs) = ProjectDirs::from("", "", crate::APP_NAME) {
        let data_dir = dirs.data_dir();
        std::fs::create_dir_all(data_dir).ok();
        data_dir.join("setbreak.db")
    } else {
        // Fallback: current directory
        PathBuf::from("setbreak.db")
    }
}

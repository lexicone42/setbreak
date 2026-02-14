pub mod analyzer;
pub mod config;
pub mod db;
pub mod scanner;

/// Audio file extensions we support
pub const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "shn", "wav"];

/// Application name for XDG paths
pub const APP_NAME: &str = "setbreak";

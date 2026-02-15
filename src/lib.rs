pub mod analyzer;
pub mod chains;
pub mod config;
pub mod db;
pub mod discovery;
pub mod scanner;
pub mod setlist;
pub mod similarity;

/// Audio file extensions we support
pub const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "shn", "wav"];

/// Application name for XDG paths
pub const APP_NAME: &str = "setbreak";

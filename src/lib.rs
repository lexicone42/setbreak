pub mod analyzer;
pub mod bands;
pub mod calibrate;
pub mod chains;
pub mod config;
pub mod db;
pub mod discovery;
pub mod scanner;
pub mod setlist;
pub mod similarity;

/// Audio file extensions we support
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "shn", "wav",
    // Native (symphonia via ferrous-waves)
    "aif", "aiff",
    // Native (dedicated Rust crates)
    "ape",
    // Native (symphonia) or ffmpeg fallback
    "wv", "m4a", "aac", "opus", "dsf", "dff",
];

/// Application name for XDG paths
pub const APP_NAME: &str = "setbreak";

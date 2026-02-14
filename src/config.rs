use directories::ProjectDirs;
use std::path::PathBuf;

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

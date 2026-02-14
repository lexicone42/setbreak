use ferrous_waves::AudioFile;
use std::path::Path;
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DecodeError {
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("ferrous-waves decode error: {0}")]
    FerrousWaves(String),
    #[error("ffmpeg not found — required for SHN files")]
    FfmpegNotFound,
    #[error("ffmpeg decode error: {0}")]
    Ffmpeg(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Load an audio file, using ferrous-waves for standard formats
/// and ffmpeg subprocess for SHN.
pub fn load_audio(path: &Path) -> Result<AudioFile, DecodeError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "shn" {
        load_shn_via_ffmpeg(path)
    } else {
        AudioFile::load(path).map_err(|e| DecodeError::FerrousWaves(e.to_string()))
    }
}

/// Decode a .shn file by shelling out to ffmpeg and converting to WAV in a temp file.
fn load_shn_via_ffmpeg(path: &Path) -> Result<AudioFile, DecodeError> {
    // Check ffmpeg is available
    let ffmpeg_check = Command::new("ffmpeg").arg("-version").output();
    if ffmpeg_check.is_err() {
        return Err(DecodeError::FfmpegNotFound);
    }

    // Create a temp file for the WAV output
    let tmp_dir = std::env::temp_dir();
    let tmp_wav = tmp_dir.join(format!("setbreak_shn_{}.wav", std::process::id()));

    let output = Command::new("ffmpeg")
        .args([
            "-i",
            &path.to_string_lossy(),
            "-f",
            "wav",
            "-acodec",
            "pcm_s16le",
            "-y",
            &tmp_wav.to_string_lossy(),
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Clean up temp file
        std::fs::remove_file(&tmp_wav).ok();
        return Err(DecodeError::Ffmpeg(stderr.to_string()));
    }

    // Load the resulting WAV with ferrous-waves
    let audio = AudioFile::load(&tmp_wav).map_err(|e| DecodeError::FerrousWaves(e.to_string()));

    // Clean up temp file
    std::fs::remove_file(&tmp_wav).ok();

    audio
}

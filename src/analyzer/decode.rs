use ferrous_waves::audio::{AudioBuffer, AudioFormat};
use ferrous_waves::AudioFile;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DecodeError {
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("ferrous-waves decode error: {0}")]
    FerrousWaves(String),
    #[error("FLAC decode error: {0}")]
    Flac(String),
    #[error("SHN decode error: {0}")]
    Shn(String),
    #[error("ffmpeg not found — required for SHN files")]
    FfmpegNotFound,
    #[error("ffmpeg decode error: {0}")]
    Ffmpeg(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Load an audio file, using the best available decoder for each format:
/// - WAV/MP3: ferrous-waves (symphonia)
/// - FLAC: claxon (native Rust, no external deps)
/// - SHN/OGG/etc: ffmpeg subprocess
pub fn load_audio(path: &Path) -> Result<AudioFile, DecodeError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "wav" | "mp3" => {
            AudioFile::load(path).map_err(|e| DecodeError::FerrousWaves(e.to_string()))
        }
        "flac" => load_flac_native(path),
        "shn" => load_shn_native(path),
        _ => load_via_ffmpeg(path),
    }
}

/// Decode a FLAC file natively using claxon, bypassing ferrous-waves's symphonia
/// decoder (which fails with "Unsupported sample format" on FLAC).
fn load_flac_native(path: &Path) -> Result<AudioFile, DecodeError> {
    let mut reader = claxon::FlacReader::open(path)
        .map_err(|e| DecodeError::Flac(format!("{}: {}", path.display(), e)))?;

    let info = reader.streaminfo();
    let sample_rate = info.sample_rate;
    let channels = info.channels as usize;
    let bits_per_sample = info.bits_per_sample;
    let scale = 2_f32.powi(bits_per_sample as i32 - 1);

    let samples_i32: Vec<i32> = reader
        .samples()
        .collect::<Result<Vec<i32>, _>>()
        .map_err(|e| DecodeError::Flac(format!("{}: {}", path.display(), e)))?;

    let samples_f32: Vec<f32> = samples_i32.iter().map(|&s| s as f32 / scale).collect();

    let buffer = AudioBuffer::new(samples_f32, sample_rate, channels);
    Ok(AudioFile {
        buffer,
        format: AudioFormat::from_path(path),
        path: path.display().to_string(),
    })
}

/// Decode a Shorten (SHN) file natively using the shorten-rs crate.
fn load_shn_native(path: &Path) -> Result<AudioFile, DecodeError> {
    let mut reader = shorten_rs::ShnReader::open(path)
        .map_err(|e| DecodeError::Shn(format!("{}: {}", path.display(), e)))?;

    let info = reader.info();
    let sample_rate = info.sample_rate;
    let channels = info.channels as usize;
    let bits_per_sample = info.bits_per_sample;
    let scale = 2_f32.powi(bits_per_sample as i32 - 1);

    let samples_i32: Vec<i32> = reader
        .samples()
        .collect::<Result<Vec<i32>, _>>()
        .map_err(|e| DecodeError::Shn(format!("{}: {}", path.display(), e)))?;

    let samples_f32: Vec<f32> = samples_i32.iter().map(|&s| s as f32 / scale).collect();

    let buffer = AudioBuffer::new(samples_f32, sample_rate, channels);
    Ok(AudioFile {
        buffer,
        format: AudioFormat::from_path(path),
        path: path.display().to_string(),
    })
}

/// Decode an audio file by shelling out to ffmpeg and converting to WAV in a temp file.
/// Works with any format ffmpeg supports (SHN, OGG, AIFF, etc.).
fn load_via_ffmpeg(path: &Path) -> Result<AudioFile, DecodeError> {
    // Check ffmpeg is available
    let ffmpeg_check = Command::new("ffmpeg").arg("-version").output();
    if ffmpeg_check.is_err() {
        return Err(DecodeError::FfmpegNotFound);
    }

    // Unique temp file per call — atomic counter avoids race with parallel rayon workers
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_dir = std::env::temp_dir();
    let tmp_wav = tmp_dir.join(format!("setbreak_decode_{}_{}.wav", std::process::id(), id));

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

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
    #[error("APE decode error: {0}")]
    Ape(String),
    #[error("ffmpeg not found — required for SHN files")]
    FfmpegNotFound,
    #[error("ffmpeg decode error: {0}")]
    Ffmpeg(String),
    #[error("DTS bitstream detected — not decodable as PCM")]
    DtsBitstream,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Load an audio file, using the best available decoder for each format:
/// - WAV/MP3/AIFF/OGG/M4A/AAC/OPUS: ferrous-waves (symphonia — all codecs compiled in)
/// - FLAC: claxon (native Rust, no external deps)
/// - SHN: shorten-rs (native Rust)
/// - APE: ape-rs (native Rust)
/// - WV/DSD/etc: ffmpeg subprocess (no native Rust decoder available)
pub fn load_audio(path: &Path) -> Result<AudioFile, DecodeError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let audio = match ext.as_str() {
        // Native via symphonia (compiled into ferrous-waves with features=["all"])
        "wav" | "mp3" | "aif" | "aiff" | "ogg" | "m4a" | "aac" | "opus" => {
            AudioFile::load(path).map_err(|e| DecodeError::FerrousWaves(e.to_string()))?
        }
        // Native via dedicated Rust crates
        "flac" => load_flac_native(path)?,
        "shn" => load_shn_native(path)?,
        "ape" => load_ape_native(path)?,
        // Fallback to ffmpeg for formats without Rust decoders (WavPack, DSD)
        _ => load_via_ffmpeg(path)?,
    };

    // Check for DTS bitstream masquerading as PCM
    if is_dts_bitstream(&audio) {
        return Err(DecodeError::DtsBitstream);
    }

    Ok(audio)
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

/// Decode a Monkey's Audio (APE) file natively using the ape-rs crate.
fn load_ape_native(path: &Path) -> Result<AudioFile, DecodeError> {
    let mut reader = ape_rs::ApeReader::open(path)
        .map_err(|e| DecodeError::Ape(format!("{}: {}", path.display(), e)))?;

    let info = reader.info();
    let sample_rate = info.sample_rate;
    let channels = info.channels as usize;
    let bits_per_sample = info.bits_per_sample;
    let scale = 2_f32.powi(bits_per_sample as i32 - 1);

    let samples_i32: Vec<i32> = reader
        .samples()
        .collect::<Result<Vec<i32>, _>>()
        .map_err(|e| DecodeError::Ape(format!("{}: {}", path.display(), e)))?;

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

/// Check if decoded audio is actually a DTS bitstream masquerading as PCM.
///
/// DTS bitstreams have a sync word `0x7FFE8001` that appears in the first
/// few kilobytes when decoded as 16-bit PCM. In floating-point samples,
/// this manifests as specific near-max-amplitude patterns.
///
/// A simpler heuristic: DTS-as-PCM sounds like white noise, so check if the
/// first few thousand samples have extremely high variance with values
/// constantly near ±1.0 (the hallmark of a bitstream interpreted as audio).
fn is_dts_bitstream(audio: &AudioFile) -> bool {
    let samples = &audio.buffer.samples;
    if samples.len() < 4096 {
        return false;
    }

    // Check the first 4096 samples for DTS characteristics:
    // 1. High proportion of near-max-amplitude values (> 0.9 or < -0.9)
    // 2. Mean very close to 0 (random-looking)
    let check = &samples[..4096];
    let near_max = check.iter().filter(|&&s| s.abs() > 0.9).count();
    let near_max_ratio = near_max as f64 / check.len() as f64;

    // DTS bitstreams typically have >30% of samples near max amplitude
    // Real audio almost never exceeds 10% in the first few seconds
    near_max_ratio > 0.25
}

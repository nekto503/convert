//! Audio Studio: конвертация и обработка аудио через FFmpeg.

use crate::process::require_engine;
use convert_core::error::{ConvertError, ConvertResult};
use convert_core::output::{build_output_path, OverwriteMode};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Целевой аудиоформат.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioOutputFormat {
    /// Расширение: mp3, wav, flac, ogg, opus, m4a, aac.
    pub container: String,
}

/// Задание на обработку аудио.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioJob {
    pub source: PathBuf,
    pub format: AudioOutputFormat,
    pub output_dir: Option<PathBuf>,
    pub suffix: Option<String>,
    pub overwrite: OverwriteMode,
    /// Битрейт, кбит/с (для lossy-форматов).
    pub bitrate_kbps: Option<u64>,
    /// Частота дискретизации, Гц.
    pub sample_rate_hz: Option<u32>,
    /// Количество каналов: 1 = моно, 2 = стерео.
    pub channels: Option<u8>,
    /// Изменение громкости в дБ (−30..=+30).
    pub volume_db: Option<f32>,
    /// Нормализация loudnorm (EBU R128).
    pub normalize: bool,
    /// Обрезка по времени, секунды.
    pub trim_start: Option<f64>,
    pub trim_end: Option<f64>,
}

impl Default for AudioJob {
    fn default() -> Self {
        Self {
            source: PathBuf::new(),
            format: AudioOutputFormat { container: "mp3".into() },
            output_dir: None,
            suffix: None,
            overwrite: OverwriteMode::Rename,
            bitrate_kbps: Some(192),
            sample_rate_hz: None,
            channels: None,
            volume_db: None,
            normalize: false,
            trim_start: None,
            trim_end: None,
        }
    }
}

/// Кодек по расширению контейнера.
fn audio_codec(container: &str) -> ConvertResult<&'static str> {
    Ok(match container {
        "mp3" => "libmp3lame",
        "wav" => "pcm_s16le",
        "flac" => "flac",
        "ogg" => "libvorbis",
        "opus" => "libopus",
        "m4a" | "aac" => "aac",
        other => {
            return Err(ConvertError::InvalidParams(format!(
                "аудиоформат {other} не поддерживается"
            )))
        }
    })
}

/// Запускает задание обработки аудио.
pub fn run_audio_job(job: &AudioJob) -> ConvertResult<PathBuf> {
    // Валидация параметров до всего остального (в т.ч. до движков).
    let codec = audio_codec(&job.format.container)?;
    let ffmpeg = require_engine("ffmpeg")?;
    if !job.source.is_file() {
        return Err(ConvertError::FileNotFound(job.source.display().to_string()));
    }

    let output = build_output_path(
        &job.source,
        job.output_dir.as_deref(),
        Some(&job.format.container),
        job.suffix.as_deref(),
        job.overwrite,
    )?;

    let mut args: Vec<String> = vec!["-y".into()];
    if let Some(start) = job.trim_start {
        args.push("-ss".into());
        args.push(format!("{start:.3}"));
    }
    if let Some(end) = job.trim_end {
        args.push("-to".into());
        args.push(format!("{end:.3}"));
    }
    args.push("-i".into());
    args.push(job.source.to_string_lossy().to_string());
    args.push("-vn".into()); // игнорировать видео-дорожки
    args.push("-c:a".into());
    args.push(codec.into());

    if let Some(kbps) = job.bitrate_kbps {
        if !matches!(job.format.container.as_str(), "wav" | "flac") {
            args.push("-b:a".into());
            args.push(format!("{}k", kbps.clamp(8, 3200)));
        }
    }
    if let Some(hz) = job.sample_rate_hz {
        args.push("-ar".into());
        args.push(format!("{}", hz.clamp(8000, 192_000)));
    }
    if let Some(ch) = job.channels {
        if ch == 1 || ch == 2 {
            args.push("-ac".into());
            args.push(format!("{ch}"));
        }
    }
    // Фильтры: громкость и/или нормализация.
    let mut filters = Vec::new();
    if let Some(db) = job.volume_db {
        filters.push(format!("volume={:.2}dB", db.clamp(-30.0, 30.0)));
    }
    if job.normalize {
        filters.push("loudnorm=I=-16:TP=-1.5:LRA=11".into());
    }
    if !filters.is_empty() {
        args.push("-af".into());
        args.push(filters.join(","));
    }
    args.push(output.to_string_lossy().to_string());

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    crate::process::run_process_with_timeout(
        ffmpeg.to_str().unwrap(),
        &arg_refs,
        Duration::from_secs(60 * 60),
    )?;
    Ok(output)
}

/// Быстрая нормализация отдельным вызовом (для контекстного меню).
pub fn normalize_audio(source: &Path, output_dir: Option<&Path>) -> ConvertResult<PathBuf> {
    let job = AudioJob {
        source: source.to_path_buf(),
        format: AudioOutputFormat {
            // Сохраняем исходное расширение, если это аудиоформат.
            container: source
                .extension()
                .and_then(|e| e.to_str())
                .map(String::from)
                .unwrap_or_else(|| "wav".into()),
        },
        output_dir: output_dir.map(Path::to_path_buf),
        normalize: true,
        ..Default::default()
    };
    run_audio_job(&job)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Генерирует тестовый тон 2 секунды через ffmpeg lavfi.
    fn make_test_audio(path: &Path) {
        let ffmpeg = crate::process::find_engine("ffmpeg")
            .expect("ffmpeg нужен для тестов convert-media");
        crate::process::run_process_with_timeout(
            ffmpeg.to_str().unwrap(),
            &[
                "-y",
                "-f", "lavfi",
                "-i", "sine=frequency=440:duration=2",
                "-c:a", "libmp3lame",
                "-b:a", "128k",
                path.to_str().unwrap(),
            ],
            Duration::from_secs(60),
        )
        .expect("не удалось создать тестовый аудио");
    }

    fn ffmpeg_available() -> bool {
        crate::process::find_engine("ffmpeg").is_some()
    }

    #[test]
    fn convert_mp3_to_wav_and_flac() {
        if !ffmpeg_available() {
            eprintln!("ffmpeg недоступен — пропуск");
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_audio1");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("tone.mp3");
        make_test_audio(&src);

        for target in ["wav", "flac"] {
            let job = AudioJob {
                source: src.clone(),
                format: AudioOutputFormat { container: target.into() },
                ..Default::default()
            };
            let out = run_audio_job(&job).unwrap();
            assert!(out.exists());
            let info = crate::probe::probe_media(&out).unwrap();
            let a = info.audio.expect("аудиопоток должен быть");
            let expected_codec = if target == "wav" { "pcm_s16le" } else { "flac" };
            assert_eq!(a.codec, expected_codec);
            assert!((info.duration_seconds - 2.0).abs() < 0.3);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn volume_and_mono() {
        if !ffmpeg_available() {
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_audio2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("tone.mp3");
        make_test_audio(&src);

        let job = AudioJob {
            source: src.clone(),
            format: AudioOutputFormat { container: "mp3".into() },
            volume_db: Some(-6.0),
            channels: Some(1),
            bitrate_kbps: Some(96),
            ..Default::default()
        };
        let out = run_audio_job(&job).unwrap();
        let info = crate::probe::probe_media(&out).unwrap();
        let a = info.audio.unwrap();
        assert_eq!(a.channels, 1, "должен быть моно");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalize_creates_file() {
        if !ffmpeg_available() {
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_audio3");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("tone.mp3");
        make_test_audio(&src);

        let out = normalize_audio(&src, None).unwrap();
        assert!(out.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trim_audio() {
        if !ffmpeg_available() {
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_audio4");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("tone.mp3");
        make_test_audio(&src);

        let job = AudioJob {
            source: src,
            format: AudioOutputFormat { container: "wav".into() },
            trim_start: Some(0.5),
            trim_end: Some(1.5),
            ..Default::default()
        };
        let out = run_audio_job(&job).unwrap();
        let info = crate::probe::probe_media(&out).unwrap();
        assert!(
            (info.duration_seconds - 1.0).abs() < 0.2,
            "длительность {} вместо ~1 c",
            info.duration_seconds
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_format_rejected() {
        let job = AudioJob {
            source: PathBuf::from("x.mp3"),
            format: AudioOutputFormat { container: "wma".into() },
            ..Default::default()
        };
        assert!(matches!(
            run_audio_job(&job),
            Err(ConvertError::InvalidParams(_))
        ));
    }
}

//! Video Studio: конвертация и обработка видео через FFmpeg.
//!
//! Возможности: выбор контейнера и кодека, разрешение, FPS, битрейт,
//! CRF-качество, обрезка по времени, извлечение кадров, скриншот,
//! склейка файлов (concat demuxer), извлечение аудиодорожки.

use crate::process::require_engine;
use convert_core::error::{ConvertError, ConvertResult};
use convert_core::output::{build_output_path, OverwriteMode};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Целевой видеоформат (контейнер + кодек).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoOutputFormat {
    /// Расширение контейнера: mp4, mkv, webm, mov, avi.
    pub container: String,
    /// Видеокодек: h264, hevc, av1, vp9, copy.
    pub video_codec: String,
    /// Аудиокодек: aac, opus, mp3, copy.
    pub audio_codec: String,
}

impl VideoOutputFormat {
    pub fn mp4() -> Self {
        Self {
            container: "mp4".into(),
            video_codec: "h264".into(),
            audio_codec: "aac".into(),
        }
    }

    pub fn mkv() -> Self {
        Self {
            container: "mkv".into(),
            video_codec: "hevc".into(),
            audio_codec: "opus".into(),
        }
    }

    pub fn webm() -> Self {
        Self {
            container: "webm".into(),
            video_codec: "vp9".into(),
            audio_codec: "opus".into(),
        }
    }
}

/// Обрезка по времени, секунды.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: Option<f64>,
    pub end: Option<f64>,
}

/// Задание на обработку видео.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoJob {
    pub source: PathBuf,
    pub format: VideoOutputFormat,
    pub output_dir: Option<PathBuf>,
    pub suffix: Option<String>,
    pub overwrite: OverwriteMode,
    /// Разрешение по ширине (высота — по пропорциям), None = как есть.
    pub width: Option<u32>,
    /// Ограничение высоты (используется вместе с width: меньшее).
    pub height: Option<u32>,
    /// Частота кадров.
    pub fps: Option<f32>,
    /// Битрейт видео, кбит/с (приоритетнее crf).
    pub video_bitrate_kbps: Option<u64>,
    /// Качество CRF: 18 (почти без потерь) ..= 32 (сильно сжато).
    /// Игнорируется, если задан video_bitrate_kbps.
    pub crf: Option<u8>,
    /// Битрейт аудио, кбит/с.
    pub audio_bitrate_kbps: Option<u64>,
    /// Убрать звук.
    pub mute: bool,
    /// Обрезка по времени.
    pub trim: Option<TimeRange>,
    /// Предустановка скорости кодирования (ultrafast..veryslow).
    pub preset: String,
}

impl Default for VideoJob {
    fn default() -> Self {
        Self {
            source: PathBuf::new(),
            format: VideoOutputFormat::mp4(),
            output_dir: None,
            suffix: None,
            overwrite: OverwriteMode::Rename,
            width: None,
            height: None,
            fps: None,
            video_bitrate_kbps: None,
            crf: Some(23),
            audio_bitrate_kbps: Some(128),
            mute: false,
            trim: None,
            preset: "medium".into(),
        }
    }
}

/// Запускает задание обработки видео.
pub fn run_video_job(job: &VideoJob) -> ConvertResult<PathBuf> {
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
    if let Some(trim) = &job.trim {
        if let Some(start) = trim.start {
            args.push("-ss".into());
            args.push(format!("{start:.3}"));
        }
        if let Some(end) = trim.end {
            args.push("-to".into());
            args.push(format!("{end:.3}"));
        }
    }
    args.push("-i".into());
    args.push(job.source.to_string_lossy().to_string());

    // Масштаб: вписать в рамку width×height с сохранением пропорций.
    if job.width.is_some() || job.height.is_some() {
        let w = job.width.unwrap_or(i32::MAX as u32).min(16384);
        let h = job.height.unwrap_or(i32::MAX as u32).min(16384);
        args.push("-vf".into());
        args.push(format!(
            "scale=w={w}:h={h}:force_original_aspect_ratio=decrease"
        ));
    }
    if let Some(fps) = job.fps {
        if fps > 0.0 && fps <= 240.0 {
            args.push("-r".into());
            args.push(format!("{fps}"));
        }
    }

    // Видеокодек.
    match job.format.video_codec.as_str() {
        "copy" => {
            args.push("-c:v".into());
            args.push("copy".into());
        }
        "h264" => {
            args.push("-c:v".into());
            args.push("libx264".into());
            args.push("-preset".into());
            args.push(job.preset.clone());
            args.push("-pix_fmt".into());
            args.push("yuv420p".into()); // совместимость с плеерами
        }
        "hevc" | "h265" => {
            args.push("-c:v".into());
            args.push("libx265".into());
            args.push("-preset".into());
            args.push(job.preset.clone());
            args.push("-pix_fmt".into());
            args.push("yuv420p".into());
        }
        "vp9" => {
            args.push("-c:v".into());
            args.push("libvpx-vp9".into());
        }
        "av1" => {
            args.push("-c:v".into());
            args.push("libsvtav1".into());
        }
        other => {
            return Err(ConvertError::InvalidParams(format!(
                "неизвестный видеокодек: {other}"
            )));
        }
    }

    // Битрейт или CRF.
    if let Some(kbps) = job.video_bitrate_kbps {
        args.push("-b:v".into());
        args.push(format!("{kbps}k"));
        args.push("-maxrate".into());
        args.push(format!("{}k", (kbps as f64 * 1.45) as u64));
        args.push("-bufsize".into());
        args.push(format!("{}k", (kbps as f64 * 2.0) as u64));
    } else if let Some(crf) = job.crf {
        args.push("-crf".into());
        args.push(format!("{}", crf.clamp(0, 51)));
    }

    // Аудио.
    if job.mute {
        args.push("-an".into());
    } else {
        args.push("-c:a".into());
        // «opus» в ffmpeg называется кодеком libopus.
        let audio_encoder = match job.format.audio_codec.as_str() {
            "opus" => "libopus",
            "vorbis" => "libvorbis",
            other => other,
        };
        args.push(audio_encoder.into());
        if let Some(kbps) = job.audio_bitrate_kbps {
            args.push("-b:a".into());
            args.push(format!("{kbps}k"));
        }
        // libopus в Matroska/WebM требует 48 кГц.
        if matches!(job.format.audio_codec.as_str(), "opus" | "libopus") {
            args.push("-ar".into());
            args.push("48000".into());
        }
    }

    // Быстрый старт для веб-плееров (только mp4/mov).
    if matches!(job.format.container.as_str(), "mp4" | "mov") {
        args.push("-movflags".into());
        args.push("+faststart".into());
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

/// Извлекает аудиодорожку из видео (mp3/wav/flac/...).
pub fn extract_audio(
    video: &Path,
    audio_format: &str,
    output_dir: Option<&Path>,
) -> ConvertResult<PathBuf> {
    let ffmpeg = require_engine("ffmpeg")?;
    let output = build_output_path(video, output_dir, Some(audio_format), None, OverwriteMode::Rename)?;
    let args = [
        "-y",
        "-i",
        video.to_str().unwrap(),
        "-vn", // без видео
        "-c:a",
        match audio_format {
            "wav" => "pcm_s16le",
            "flac" => "flac",
            "mp3" => "libmp3lame",
            "opus" => "libopus",
            "aac" => "aac",
            "ogg" => "libvorbis",
            other => return Err(ConvertError::InvalidParams(format!("аудиокодек {other} не поддерживается"))),
        },
        output.to_str().unwrap(),
    ];
    crate::process::run_process_with_timeout(
        ffmpeg.to_str().unwrap(),
        &args,
        Duration::from_secs(60 * 60),
    )?;
    Ok(output)
}

/// Извлекает кадры видео как изображения PNG (каждый N-й кадр).
pub fn extract_frames(
    video: &Path,
    every_nth: u32,
    output_dir: &Path,
) -> ConvertResult<Vec<PathBuf>> {
    let ffmpeg = require_engine("ffmpeg")?;
    if every_nth == 0 {
        return Err(ConvertError::InvalidParams("шаг кадров должен быть > 0".into()));
    }
    std::fs::create_dir_all(output_dir)?;
    // Уникальный префикс по имени файла.
    let stem = video
        .file_stem()
        .and_then(|s| s.to_str())
        .map(convert_core::output::sanitize_filename)
        .unwrap_or_else(|| "frame".into());
    let pattern = output_dir
        .join(format!("{stem}_%04d.png"))
        .to_string_lossy()
        .to_string();
    let args = [
        "-y".to_string(),
        "-i".to_string(),
        video.to_string_lossy().to_string(),
        "-vf".to_string(),
        format!("select=not(mod(n\\,{every_nth}))"),
        "-vsync".to_string(),
        "vfr".to_string(),
        pattern,
    ];
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    crate::process::run_process_with_timeout(
        ffmpeg.to_str().unwrap(),
        &arg_refs,
        Duration::from_secs(60 * 60),
    )?;

    // Собираем созданные файлы.
    let mut frames = Vec::new();
    if let Ok(entries) = std::fs::read_dir(output_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("png")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with(&stem))
                    .unwrap_or(false)
            {
                frames.push(p);
            }
        }
    }
    frames.sort();
    Ok(frames)
}

/// Скриншот видео в момент времени t (секунды).
pub fn screenshot(video: &Path, at_second: f64, output: &Path) -> ConvertResult<()> {
    let ffmpeg = require_engine("ffmpeg")?;
    let args = [
        "-y",
        "-ss",
        &format!("{at_second:.3}"),
        "-i",
        video.to_str().unwrap(),
        "-frames:v",
        "1",
        output.to_str().unwrap(),
    ];
    crate::process::run_process_with_timeout(
        ffmpeg.to_str().unwrap(),
        &args,
        Duration::from_secs(120),
    )?;
    Ok(())
}

/// Склейка видео одинакового формата (concat demuxer, без перекодирования).
pub fn concat_videos(videos: &[PathBuf], output: &Path) -> ConvertResult<PathBuf> {
    if videos.len() < 2 {
        return Err(ConvertError::InvalidParams(
            "для склейки нужно минимум два файла".into(),
        ));
    }
    let ffmpeg = require_engine("ffmpeg")?;

    // concat-список пишем во временный файл.
    let temp_dir = std::env::temp_dir();
    let list_path = temp_dir.join(format!("convert_concat_{}.txt", std::process::id()));
    let mut list = String::new();
    for v in videos {
        // Экранирование одинарных кавычек для concat-формата.
        let escaped = v.to_string_lossy().replace('\'', "'\\''");
        list.push_str(&format!("file '{escaped}'\n"));
    }
    std::fs::write(&list_path, &list)?;

    let args = [
        "-y",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
        list_path.to_str().unwrap(),
        "-c",
        "copy",
        output.to_str().unwrap(),
    ];
    let result = crate::process::run_process_with_timeout(
        ffmpeg.to_str().unwrap(),
        &args,
        Duration::from_secs(60 * 60),
    );
    let _ = std::fs::remove_file(&list_path);
    result?;
    Ok(output.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::tests::make_test_video;

    fn ffmpeg_available() -> bool {
        crate::process::find_engine("ffmpeg").is_some()
    }

    #[test]
    fn convert_mp4_to_mkv() {
        if !ffmpeg_available() {
            eprintln!("ffmpeg недоступен — пропуск");
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_video1");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("test.mp4");
        make_test_video(&src);

        let job = VideoJob {
            source: src.clone(),
            format: VideoOutputFormat::mkv(),
            ..Default::default()
        };
        let out = run_video_job(&job).unwrap();
        assert!(out.exists());
        assert_eq!(out.extension().unwrap(), "mkv");

        // Проверяем результат зондом.
        let info = crate::probe::probe_media(&out).unwrap();
        assert!(info.video.is_some());
        assert!((info.duration_seconds - 2.0).abs() < 1.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resize_and_trim() {
        if !ffmpeg_available() {
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_video2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("test.mp4");
        make_test_video(&src);

        let job = VideoJob {
            source: src.clone(),
            format: VideoOutputFormat::mp4(),
            width: Some(640),
            height: Some(640),
            trim: Some(TimeRange { start: Some(0.0), end: Some(1.0) }),
            crf: Some(28),
            preset: "ultrafast".into(),
            ..Default::default()
        };
        let out = run_video_job(&job).unwrap();
        let info = crate::probe::probe_media(&out).unwrap();
        let v = info.video.unwrap();
        assert_eq!(v.width, 640);
        assert_eq!(v.height, 360); // 16:9 → 640x360
        assert!(info.duration_seconds < 1.6, "обрезка не сработала: {}", info.duration_seconds);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_audio_from_video() {
        if !ffmpeg_available() {
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_video3");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("test.mp4");
        make_test_video(&src);

        let out = extract_audio(&src, "mp3", None).unwrap();
        assert!(out.exists());
        let info = crate::probe::probe_media(&out).unwrap();
        assert!(info.audio.is_some());
        assert_eq!(info.audio.unwrap().codec, "mp3");
        assert!(info.video.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn frames_and_screenshot() {
        if !ffmpeg_available() {
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_video4");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("test.mp4");
        make_test_video(&src);

        let frames_dir = dir.join("frames");
        let frames = extract_frames(&src, 10, &frames_dir).unwrap();
        assert!(!frames.is_empty(), "кадры не извлечены");
        for f in &frames {
            assert!(image::open(f).is_ok(), "кадр {} повреждён", f.display());
        }

        let shot = dir.join("shot.png");
        screenshot(&src, 1.0, &shot).unwrap();
        assert!(image::open(&shot).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn concat_two_videos() {
        if !ffmpeg_available() {
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_video5");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.mp4");
        let b = dir.join("b.mp4");
        make_test_video(&a);
        make_test_video(&b);

        let out = dir.join("joined.mp4");
        concat_videos(&[a, b], &out).unwrap();
        let info = crate::probe::probe_media(&out).unwrap();
        assert!(
            (info.duration_seconds - 4.0).abs() < 1.2,
            "длительность {} вместо ~4 c",
            info.duration_seconds
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mute_and_fps() {
        if !ffmpeg_available() {
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_video6");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("test.mp4");
        make_test_video(&src);

        let job = VideoJob {
            source: src.clone(),
            format: VideoOutputFormat::mp4(),
            mute: true,
            fps: Some(12.0),
            preset: "ultrafast".into(),
            ..Default::default()
        };
        let out = run_video_job(&job).unwrap();
        let info = crate::probe::probe_media(&out).unwrap();
        assert!(info.audio.is_none(), "звук должен быть удалён");
        let v = info.video.unwrap();
        assert!((v.fps - 12.0).abs() < 1.0, "fps = {}", v.fps);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_unknown_codec() {
        let job = VideoJob {
            source: PathBuf::from("x.mp4"),
            format: VideoOutputFormat {
                container: "mp4".into(),
                video_codec: "magic".into(),
                audio_codec: "aac".into(),
            },
            ..Default::default()
        };
        assert!(run_video_job(&job).is_err() || !Path::new("x.mp4").exists());
    }
}

//! Разбор технических характеристик медиафайлов через ffprobe
//! (Smart Scan, Video Studio, File Compare).

use crate::process::require_engine;
use convert_core::error::{ConvertError, ConvertResult};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

/// Видеопоток.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VideoStream {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub bitrate_kbps: Option<u64>,
    pub pixel_format: Option<String>,
}

/// Аудиопоток.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AudioStream {
    pub codec: String,
    pub sample_rate_hz: u32,
    pub channels: u8,
    pub bitrate_kbps: Option<u64>,
}

/// Сводка медиафайла.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MediaInfo {
    pub duration_seconds: f64,
    pub format_name: String,
    pub size_bytes: u64,
    pub overall_bitrate_kbps: Option<u64>,
    pub video: Option<VideoStream>,
    pub audio: Option<AudioStream>,
    pub tags: std::collections::HashMap<String, String>,
}

impl MediaInfo {
    /// Человекочитаемая длительность MM:SS.
    pub fn duration_label(&self) -> String {
        let total = self.duration_seconds as u64;
        format!("{}:{:02}", total / 60, total % 60)
    }
}

/// Спрашивает у ffprobe всё о файле (JSON).
pub fn probe_media(path: &Path) -> ConvertResult<MediaInfo> {
    let ffprobe = require_engine("ffprobe")?;
    let path_str = path.to_str().ok_or_else(|| {
        ConvertError::InvalidParams("путь содержит не-UTF8 символы".into())
    })?;

    let output = crate::process::run_process_with_timeout(
        ffprobe.to_str().unwrap(),
        &[
            "-v", "error",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            path_str,
        ],
        Duration::from_secs(60),
    )?;

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| ConvertError::Processing(format!("ffprobe вернул некорректный JSON: {e}")))?;

    parse_probe_json(&json, path)
}

fn parse_probe_json(json: &serde_json::Value, path: &Path) -> ConvertResult<MediaInfo> {
    let format = json.get("format").cloned().unwrap_or_default();
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    let mut info = MediaInfo {
        duration_seconds: format
            .get("duration")
            .and_then(|d| d.as_str())
            .and_then(|d| d.parse().ok())
            .unwrap_or(0.0),
        format_name: format
            .get("format_name")
            .and_then(|f| f.as_str())
            .unwrap_or("unknown")
            .to_string(),
        size_bytes,
        overall_bitrate_kbps: format
            .get("bit_rate")
            .and_then(|b| b.as_str())
            .and_then(|b| b.parse::<u64>().ok())
            .map(|b| b / 1000),
        ..Default::default()
    };

    if let Some(tags) = format.get("tags").and_then(|t| t.as_object()) {
        for (k, v) in tags {
            if let Some(v) = v.as_str() {
                info.tags.insert(k.clone(), v.to_string());
            }
        }
    }

    if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
        for stream in streams {
            let codec_type = stream.get("codec_type").and_then(|t| t.as_str()).unwrap_or("");
            match codec_type {
                "video" if info.video.is_none() => {
                    info.video = Some(VideoStream {
                        codec: stream
                            .get("codec_name")
                            .and_then(|c| c.as_str())
                            .unwrap_or("?")
                            .to_string(),
                        width: stream.get("width").and_then(|w| w.as_u64()).unwrap_or(0) as u32,
                        height: stream.get("height").and_then(|h| h.as_u64()).unwrap_or(0) as u32,
                        fps: parse_fps(stream),
                        bitrate_kbps: stream
                            .get("bit_rate")
                            .and_then(|b| b.as_str())
                            .and_then(|b| b.parse::<u64>().ok())
                            .map(|b| b / 1000),
                        pixel_format: stream
                            .get("pix_fmt")
                            .and_then(|p| p.as_str())
                            .map(String::from),
                    });
                }
                "audio" if info.audio.is_none() => {
                    info.audio = Some(AudioStream {
                        codec: stream
                            .get("codec_name")
                            .and_then(|c| c.as_str())
                            .unwrap_or("?")
                            .to_string(),
                        sample_rate_hz: stream
                            .get("sample_rate")
                            .and_then(|s| s.as_str())
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0),
                        channels: stream
                            .get("channels")
                            .and_then(|c| c.as_u64())
                            .unwrap_or(0) as u8,
                        bitrate_kbps: stream
                            .get("bit_rate")
                            .and_then(|b| b.as_str())
                            .and_then(|b| b.parse::<u64>().ok())
                            .map(|b| b / 1000),
                    });
                }
                _ => {}
            }
        }
    }
    Ok(info)
}

fn parse_fps(stream: &serde_json::Value) -> f32 {
    stream
        .get("avg_frame_rate")
        .or_else(|| stream.get("r_frame_rate"))
        .and_then(|f| f.as_str())
        .and_then(|f| {
            let mut parts = f.split('/');
            let num: f32 = parts.next()?.parse().ok()?;
            let den: f32 = parts.next().unwrap_or("1").parse().ok()?;
            if den == 0.0 {
                None
            } else {
                Some(num / den)
            }
        })
        .unwrap_or(0.0)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Тестовая генерация видео через ffmpeg (lavfi): 2 сек, 1280x720, тон.
    pub fn make_test_video(path: &Path) {
        let ffmpeg = crate::process::find_engine("ffmpeg")
            .expect("ffmpeg нужен для тестов convert-media");
        crate::process::run_process_with_timeout(
            ffmpeg.to_str().unwrap(),
            &[
                "-y",
                "-f", "lavfi",
                "-i", "testsrc2=duration=2:size=1280x720:rate=25",
                "-f", "lavfi",
                "-i", "sine=frequency=440:duration=2",
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-pix_fmt", "yuv420p",
                "-c:a", "aac",
                "-shortest",
                path.to_str().unwrap(),
            ],
            Duration::from_secs(120),
        )
        .expect("не удалось создать тестовое видео");
    }

    #[test]
    fn probe_reports_streams() {
        if crate::process::find_engine("ffprobe").is_none() {
            eprintln!("ffprobe недоступен — пропуск");
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_probe");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("test.mp4");
        make_test_video(&video);

        let info = probe_media(&video).unwrap();
        let v = info.video.expect("видеопоток должен быть");
        assert_eq!((v.width, v.height), (1280, 720));
        assert_eq!(v.codec, "h264");
        assert!((v.fps - 25.0).abs() < 1.0, "fps = {}", v.fps);
        let a = info.audio.expect("аудиопоток должен быть");
        assert_eq!(a.codec, "aac");
        assert_eq!(a.sample_rate_hz, 44100);
        assert!((info.duration_seconds - 2.0).abs() < 0.5);
        assert!(info.size_bytes > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn probe_rejects_missing_file() {
        if crate::process::find_engine("ffprobe").is_none() {
            return;
        }
        let err = probe_media(Path::new("/nonexistent/file.mp4"));
        assert!(err.is_err());
    }

    #[test]
    fn duration_label() {
        let info = MediaInfo { duration_seconds: 125.0, ..Default::default() };
        assert_eq!(info.duration_label(), "2:05");
    }
}

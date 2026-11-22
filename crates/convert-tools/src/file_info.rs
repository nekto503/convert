//! File Information: подробная информация о любом файле (Smart Scan
//! из ТЗ 5.1 в базовой части + File Information из 4.11).

use convert_core::error::{ConvertError, ConvertResult};
use convert_core::format::{detect_format_path, FileCategory, FileFormat};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Сводная информация о файле.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub file_name: String,
    /// Определённый по содержимому формат.
    pub detected_format: String,
    pub category: String,
    /// Расширение файла.
    pub extension: String,
    /// Совпадает ли расширение с реальным форматом.
    pub extension_matches: bool,
    pub size_bytes: u64,
    pub modified_unix: i64,
    /// Дополнительные детали (для изображений — размеры, для медиа — кодеки).
    pub details: Vec<(String, String)>,
}

/// Собирает информацию о файле.
pub fn file_information(path: &Path) -> ConvertResult<FileInfo> {
    let meta = std::fs::metadata(path)
        .map_err(|_| ConvertError::FileNotFound(path.display().to_string()))?;
    if !meta.is_file() {
        return Err(ConvertError::NotAFile(path.display().to_string()));
    }

    let format = detect_format_path(path)?;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let extension_matches = format == FileFormat::Unknown
        || extension == format.extension()
        || matches!(
            (format, extension.as_str()),
            (FileFormat::Jpeg, "jpeg")
                | (FileFormat::Jpeg, "jpe")
                | (FileFormat::Tiff, "tif")
                | (FileFormat::Matroska, "webm")
                | (FileFormat::M4a, "m4a")
                | (FileFormat::OggVorbis, "ogg")
                | (FileFormat::PlainText, "log")
                | (FileFormat::PlainText, "md")
                | (FileFormat::PlainText, "xml")
                | (FileFormat::Svg, "svg")
        );

    let mut details = Vec::new();
    match format.category() {
        FileCategory::Image => {
            if let Ok(dims) = image::image_dimensions(path) {
                details.push(("Разрешение".into(), format!("{}×{}", dims.0, dims.1)));
            }
        }
        FileCategory::Video | FileCategory::Audio => {
            if let Ok(info) = convert_media_probe(path) {
                details.push(("Длительность".into(), info.0));
                if let Some(v) = info.1 {
                    details.push(("Видео".into(), v));
                }
                if let Some(a) = info.2 {
                    details.push(("Аудио".into(), a));
                }
            }
        }
        FileCategory::Pdf => {
            if let Ok(n) = convert_pdf::pdf_page_count(path) {
                details.push(("Страниц".into(), n.to_string()));
            }
        }
        FileCategory::Archive => {
            details.push(("Тип".into(), "Архив".into()));
        }
        _ => {}
    }

    Ok(FileInfo {
        path: path.display().to_string(),
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        detected_format: format.display_name().to_string(),
        category: category_label(format),
        extension,
        extension_matches,
        size_bytes: meta.len(),
        modified_unix: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        details,
    })
}

fn category_label(format: FileFormat) -> String {
    match format.category() {
        FileCategory::Image => "Изображение".into(),
        FileCategory::Video => "Видео".into(),
        FileCategory::Audio => "Аудио".into(),
        FileCategory::Document => "Документ".into(),
        FileCategory::Spreadsheet => "Таблица".into(),
        FileCategory::Presentation => "Презентация".into(),
        FileCategory::Pdf => "PDF".into(),
        FileCategory::Archive => "Архив".into(),
        FileCategory::Text => "Текст".into(),
        FileCategory::Font => "Шрифт".into(),
        FileCategory::Other => "Другое".into(),
    }
}

/// Медиа-инфо через ffprobe (движок convert-media; отдельная функция,
/// чтобы не тащить зависимость — вызов через динамику невозможен, поэтому
/// используем прямой вызов).
fn convert_media_probe(path: &Path) -> ConvertResult<(String, Option<String>, Option<String>)> {
    let info = convert_media::probe_media(path)?;
    let dur = format!("{:.1} c", info.duration_seconds);
    let video = info.video.map(|v| {
        format!("{} {}×{} @ {:.0} fps", v.codec, v.width, v.height, v.fps)
    });
    let audio = info
        .audio
        .map(|a| format!("{} {} Гц, {} кан.", a.codec, a.sample_rate_hz, a.channels));
    Ok((dur, video, audio))
}

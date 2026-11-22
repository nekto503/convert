//! File Compare (ТЗ §7): сравнение исходного и обработанного файлов.
//!
//! - Изображения: размеры, формат, вес, попиксельная разница (MAE),
//!   миниатюры до/после как data URL для слайдера в UI.
//! - Видео/аудио: длительность, битрейт, кодеки, разрешение (ffprobe).
//! - Текст/документы: построчный diff с контекстом.
//! - Любые файлы: хэш-совпадение (байт в байт).

use convert_core::error::{ConvertError, ConvertResult};
use image::ImageEncoder;
use convert_core::format::{detect_format_path, FileCategory};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Общая сводка сравнения.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareReport {
    pub kind: CompareKind,
    /// Одинаковое ли содержимое (байт в байт).
    pub identical: bool,
    pub left_size: u64,
    pub right_size: bool_as_u64_alias,
    /// Изменение размера: −0.4 = легче на 40%.
    pub size_change_percent: f32,
    /// Процент различия пикселей (только изображения).
    pub pixel_difference_percent: Option<f32>,
    /// Миниатюры для UI (data:image/png;base64,...).
    pub left_preview: Option<String>,
    pub right_preview: Option<String>,
    /// Построчный diff (тексты).
    pub text_diff: Vec<DiffLine>,
    /// Технические характеристики построчно.
    pub properties: Vec<(String, String, String)>,
}

/// Внутренний алиас для читаемости JSON-схемы.
type bool_as_u64_alias = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareKind {
    Image,
    Media,
    Text,
    Generic,
}

/// Строка diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DiffLine {
    pub change: DiffChange,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffChange {
    Equal,
    Added,
    Removed,
}

/// Сравнивает два файла, выбирая режим по категории.
pub fn compare_files(left: &Path, right: &Path) -> ConvertResult<CompareReport> {
    if !left.is_file() {
        return Err(ConvertError::FileNotFound(left.display().to_string()));
    }
    if !right.is_file() {
        return Err(ConvertError::FileNotFound(right.display().to_string()));
    }
    let left_size = std::fs::metadata(left)?.len();
    let right_size = std::fs::metadata(right)?.len();
    let identical = files_identical(left, right)?;
    let size_change_percent = if left_size > 0 {
        (right_size as f32 / left_size as f32 - 1.0) * 100.0
    } else {
        0.0
    };

    let mut report = CompareReport {
        kind: CompareKind::Generic,
        identical,
        left_size,
        right_size,
        size_change_percent,
        pixel_difference_percent: None,
        left_preview: None,
        right_preview: None,
        text_diff: Vec::new(),
        properties: Vec::new(),
    };

    let left_fmt = detect_format_path(left).unwrap_or(convert_core::format::FileFormat::Unknown);
    match left_fmt.category() {
        FileCategory::Image => compare_images(left, right, &mut report),
        FileCategory::Video | FileCategory::Audio => compare_media(left, right, &mut report),
        FileCategory::Text => compare_texts(left, right, &mut report),
        _ => {
            report.properties.push((
                "Тип".into(),
                "двоичный файл".into(),
                "двоичный файл".into(),
            ));
        }
    }
    Ok(report)
}

/// Побайтовое сравнение (без загрузки в память целиком).
fn files_identical(a: &Path, b: &Path) -> ConvertResult<bool> {
    use std::io::Read;
    let mut fa = std::fs::File::open(a)?;
    let mut fb = std::fs::File::open(b)?;
    let mut ba = vec![0u8; 64 * 1024];
    let mut bb = vec![0u8; 64 * 1024];
    loop {
        let na = fa.read(&mut ba)?;
        let nb = fb.read(&mut bb)?;
        if na != nb {
            return Ok(false);
        }
        if na == 0 {
            return Ok(true);
        }
        if ba[..na] != bb[..nb] {
            return Ok(false);
        }
    }
}

// ---- Изображения -----------------------------------------------------------

fn compare_images(left: &Path, right: &Path, report: &mut CompareReport) {
    report.kind = CompareKind::Image;
    let img_l = convert_image::load_image(left).ok();
    let img_r = convert_image::load_image(right).ok();
    if let (Some(l), Some(r)) = (&img_l, &img_r) {
        report.properties.push((
            "Разрешение".into(),
            format!("{}×{}", l.width(), l.height()),
            format!("{}×{}", r.width(), r.height()),
        ));
        report.pixel_difference_percent = Some(pixel_difference(l, r));
    }
    if let Some(l) = &img_l {
        report.left_preview = Some(preview_data_url(l));
    }
    if let Some(r) = &img_r {
        report.right_preview = Some(preview_data_url(r));
    }
}

/// Средняя попиксельная разница (Mean Absolute Error) в процентах.
/// Изображения приводятся к общему размеру меньшего.
fn pixel_difference(a: &image::DynamicImage, b: &image::DynamicImage) -> f32 {
    use image::GenericImageView;
    let w = a.width().min(b.width()).max(1);
    let h = a.height().min(b.height()).max(1);
    let la = a.thumbnail_exact(w, h).to_rgba8();
    let lb = b.thumbnail_exact(w, h).to_rgba8();
    let mut total: u64 = 0;
    for (pa, pb) in la.pixels().zip(lb.pixels()) {
        total += (pa[0] as i32 - pb[0] as i32).unsigned_abs() as u64;
        total += (pa[1] as i32 - pb[1] as i32).unsigned_abs() as u64;
        total += (pa[2] as i32 - pb[2] as i32).unsigned_abs() as u64;
    }
    let max = (w * h * 3 * 255) as f32;
    (total as f32 / max) * 100.0
}

/// Миниатюра 240 px как data URL (для слайдера «до/после»).
fn preview_data_url(img: &image::DynamicImage) -> String {
    use image::GenericImageView;
    let longest = img.width().max(img.height());
    let thumb = if longest > 240 {
        let scale = 240.0 / longest as f32;
        let w = ((img.width() as f32 * scale).round() as u32).max(1);
        let h = ((img.height() as f32 * scale).round() as u32).max(1);
        img.thumbnail_exact(w, h)
    } else {
        img.clone()
    };
    let mut buf = Vec::new();
    let rgb = thumb.to_rgb8();
    if image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85)
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .is_ok()
    {
        use base64_encode as _b64;
        format!("data:image/jpeg;base64,{}", b64(&buf))
    } else {
        String::new()
    }
}

/// Кодирование base64 (без внешних зависимостей).
mod base64_encode {
    pub fn encode(data: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
        for chunk in data.chunks(3) {
            let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            out.push(T[(n >> 18) as usize & 63] as char);
            out.push(T[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
            out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
        }
        out
    }
}
use base64_encode::encode as b64;

// ---- Медиа -------------------------------------------------------------------

fn compare_media(left: &Path, right: &Path, report: &mut CompareReport) {
    report.kind = CompareKind::Media;
    let info_l = convert_media::probe_media(left).ok();
    let info_r = convert_media::probe_media(right).ok();
    if let (Some(l), Some(r)) = (&info_l, &info_r) {
        report.properties.push((
            "Длительность".into(),
            format!("{:.1} с", l.duration_seconds),
            format!("{:.1} с", r.duration_seconds),
        ));
        report.properties.push((
            "Битрейт".into(),
            format!("{} кбит/с", l.overall_bitrate_kbps.unwrap_or(0)),
            format!("{} кбит/с", r.overall_bitrate_kbps.unwrap_or(0)),
        ));
        if let (Some(vl), Some(vr)) = (&l.video, &r.video) {
            report.properties.push((
                "Видео".into(),
                format!("{} {}×{} @ {:.0} fps", vl.codec, vl.width, vl.height, vl.fps),
                format!("{} {}×{} @ {:.0} fps", vr.codec, vr.width, vr.height, vr.fps),
            ));
        }
        if let (Some(al), Some(ar)) = (&l.audio, &r.audio) {
            report.properties.push((
                "Аудио".into(),
                format!("{} {} Гц {}", al.codec, al.sample_rate_hz, al.channels),
                format!("{} {} Гц {}", ar.codec, ar.sample_rate_hz, ar.channels),
            ));
        }
    }
}

// ---- Тексты -------------------------------------------------------------------

fn compare_texts(left: &Path, right: &Path, report: &mut CompareReport) {
    report.kind = CompareKind::Text;
    let l = std::fs::read_to_string(left).unwrap_or_default();
    let r = std::fs::read_to_string(right).unwrap_or_default();
    report.text_diff = text_diff(&l, &r);
    report.properties.push((
        "Строк".into(),
        l.lines().count().to_string(),
        r.lines().count().to_string(),
    ));
}

/// Простой LCS-diff по строкам (классическая DP; для файлов до ~2000 строк).
pub fn text_diff(left: &str, right: &str) -> Vec<DiffLine> {
    let a: Vec<&str> = left.lines().collect();
    let b: Vec<&str> = right.lines().collect();
    // Таблица LCS.
    let (n, m) = (a.len(), b.len());
    if n.saturating_mul(m) > 4_000_000 {
        // Слишком большой файл: честно возвращаем укороченный сводный diff.
        let mut out = Vec::new();
        for line in a.iter().take(200) {
            out.push(DiffLine { change: DiffChange::Removed, text: (*line).into() });
        }
        for line in b.iter().take(200) {
            out.push(DiffLine { change: DiffChange::Added, text: (*line).into() });
        }
        out.push(DiffLine {
            change: DiffChange::Equal,
            text: "…файл слишком большой для полного diff, показаны первые 200 строк каждой версии".into(),
        });
        return out;
    }
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    // Восстановление.
    let mut result = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            result.push(DiffLine { change: DiffChange::Equal, text: a[i].into() });
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            result.push(DiffLine { change: DiffChange::Removed, text: a[i].into() });
            i += 1;
        } else {
            result.push(DiffLine { change: DiffChange::Added, text: b[j].into() });
            j += 1;
        }
    }
    while i < n {
        result.push(DiffLine { change: DiffChange::Removed, text: a[i].into() });
        i += 1;
    }
    while j < m {
        result.push(DiffLine { change: DiffChange::Added, text: b[j].into() });
        j += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_files_detected() {
        let dir = std::env::temp_dir().join("convert_cmp1");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"same content").unwrap();
        std::fs::write(dir.join("b.txt"), b"same content").unwrap();
        let report = compare_files(&dir.join("a.txt"), &dir.join("b.txt")).unwrap();
        assert!(report.identical);
        assert_eq!(report.kind, CompareKind::Text);
        assert!(report.text_diff.iter().all(|l| l.change == DiffChange::Equal));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn text_diff_shows_changes() {
        let dir = std::env::temp_dir().join("convert_cmp2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("old.txt"), "line1\nline2\nline3\n").unwrap();
        std::fs::write(dir.join("new.txt"), "line1\nCHANGED\nline3\nline4\n").unwrap();
        let report = compare_files(&dir.join("old.txt"), &dir.join("new.txt")).unwrap();
        assert!(!report.identical);
        let added: Vec<_> = report
            .text_diff
            .iter()
            .filter(|l| l.change == DiffChange::Added)
            .collect();
        let removed: Vec<_> = report
            .text_diff
            .iter()
            .filter(|l| l.change == DiffChange::Removed)
            .collect();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].text, "line2");
        assert!(added.iter().any(|l| l.text == "CHANGED"));
        assert!(added.iter().any(|l| l.text == "line4"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn image_compare_with_previews() {
        let dir = std::env::temp_dir().join("convert_cmp3");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let img = image::RgbImage::from_fn(100, 80, |x, y| image::Rgb([(x * 2) as u8, (y * 3) as u8, 90]));
        let png = dir.join("orig.png");
        image::save_buffer_with_format(
            &png,
            img.as_raw(),
            100,
            80,
            image::ExtendedColorType::Rgb8,
            image::ImageFormat::Png,
        )
        .unwrap();
        // «Обработка»: перекодировали в JPEG (потери).
        let jpg = dir.join("compressed.jpg");
        let loaded = image::open(&png).unwrap();
        loaded.save_with_format(&jpg, image::ImageFormat::Jpeg).unwrap();

        let report = compare_files(&png, &jpg).unwrap();
        assert_eq!(report.kind, CompareKind::Image);
        assert!(!report.identical);
        // JPEG-сжатие меняет пиксели.
        let diff = report.pixel_difference_percent.unwrap();
        assert!(diff > 0.0 && diff < 30.0, "diff = {diff}");
        // Превью — data URL.
        assert!(report.left_preview.as_deref().unwrap().starts_with("data:image/jpeg;base64,"));
        assert!(report.right_preview.is_some());
        // Разрешение в свойствах.
        assert!(report.properties.iter().any(|(k, ..)| k == "Разрешение"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn generic_binary_compare() {
        let dir = std::env::temp_dir().join("convert_cmp4");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("x.bin"), vec![1u8, 2, 3, 4, 5, 6, 7, 8, 9]).unwrap();
        std::fs::write(dir.join("y.bin"), vec![1u8, 2, 3, 4, 5, 6, 7, 8, 10]).unwrap();
        let report = compare_files(&dir.join("x.bin"), &dir.join("y.bin")).unwrap();
        assert_eq!(report.kind, CompareKind::Generic);
        assert!(!report.identical);
        assert_eq!(report.left_size, 9);
        assert_eq!(report.right_size, 9);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_rejected() {
        assert!(compare_files(Path::new("/no/a.txt"), Path::new("/no/b.txt")).is_err());
    }

    #[test]
    fn base64_encoder_valid() {
        assert_eq!(b64(b"hello"), "aGVsbG8=");
        assert_eq!(b64(b"hi"), "aGk=");
        assert_eq!(b64(b"A"), "QQ==");
        assert_eq!(b64(b"AAA"), "QUFB");
    }
}

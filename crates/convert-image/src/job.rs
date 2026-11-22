//! Главный пайплайн обработки изображения: загрузка → (SVG-растеризация) →
//! кадрирование → масштаб → поворот → цветокоррекция → удаление фона →
//! сохранение с настройками. Одна структура `ImageJob` описывает всё, что
//! умеет Image Studio; `run_job` выполняет её и возвращает результат.

use crate::background::{remove_background_solid, BackgroundRemovalParams};
use crate::encode::{encode_to_file, EncodeParams, ImageOutputFormat};
use crate::metadata::{jpeg_set_dpi, png_set_dpi, read_dpi, read_image_metadata};
use crate::ops::{apply_adjustments, crop, resize, rotate, ColorAdjustments, CropParams, ResizeParams, RotationOp};
use crate::svg::rasterize_svg;
use convert_core::error::{ConvertError, ConvertResult};
use convert_core::format::detect_format_path;
use convert_core::output::{build_output_path, OverwriteMode};
use image::DynamicImage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Описание исходного файла (для Smart Scan / File Compare).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSourceInfo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub file_size: u64,
}

/// Полное задание на обработку изображения.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageJob {
    /// Исходный файл (JPEG/PNG/GIF/BMP/WEBP/TIFF/ICO/SVG).
    pub source: PathBuf,
    /// Целевой формат.
    pub target_format: ImageOutputFormat,
    /// Папка результата: None = рядом с исходником.
    pub output_dir: Option<PathBuf>,
    /// Суффикс имени файла, например "_compressed".
    pub suffix: Option<String>,
    /// Перезапись существующего результата.
    pub overwrite: OverwriteMode,
    /// Кадрирование.
    pub crop: Option<CropParams>,
    /// Масштабирование.
    pub resize: Option<ResizeParams>,
    /// Поворот/отражение.
    pub rotate: Option<RotationOp>,
    /// Цветокоррекция.
    pub adjustments: Option<ColorAdjustments>,
    /// Удаление однотонного фона (результат — формат с альфой).
    pub remove_background: Option<BackgroundRemovalParams>,
    /// Параметры кодирования (качество, DPI).
    pub encode: EncodeParams,
    /// Сохранить EXIF исходника в результат (только JPEG→JPEG).
    pub keep_exif: bool,
}

impl Default for ImageJob {
    fn default() -> Self {
        Self {
            source: PathBuf::new(),
            target_format: ImageOutputFormat::Png,
            output_dir: None,
            suffix: None,
            overwrite: OverwriteMode::Rename,
            crop: None,
            resize: None,
            rotate: None,
            adjustments: None,
            remove_background: None,
            encode: EncodeParams::default(),
            keep_exif: false,
        }
    }
}

/// Результат обработки.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageJobResult {
    pub output_path: PathBuf,
    /// Размер исходника, байт.
    pub input_size: u64,
    /// Размер результата, байт.
    pub output_size: u64,
    /// Разница размеров: −0.5 = ужали на 50%.
    pub size_change: f32,
    pub width: u32,
    pub height: u32,
    /// Время выполнения, мс.
    pub duration_ms: u128,
}

/// Читает исходное изображение; SVG растеризуется заранее.
pub fn load_image(source: &Path) -> ConvertResult<DynamicImage> {
    let fmt = detect_format_path(source)?;
    match fmt {
        convert_core::format::FileFormat::Svg => {
            rasterize_svg(source, None)
        }
        _ => image::open(source).map_err(|e| {
            ConvertError::Processing(format!("не удалось открыть {}: {e}", source.display()))
        }),
    }
}

/// Выполняет задание.
pub fn run_job(job: &ImageJob) -> ConvertResult<ImageJobResult> {
    let started = Instant::now();
    if !job.source.is_file() {
        return Err(ConvertError::FileNotFound(job.source.display().to_string()));
    }
    let input_size = std::fs::metadata(&job.source)?.len();

    // 1. Загрузка (SVG → растеризация).
    let mut img = load_image(&job.source)?;

    // 2. Кадрирование.
    if let Some(c) = &job.crop {
        img = crop(&img, c).map_err(ConvertError::InvalidParams)?;
    }
    // 3. Масштаб.
    if let Some(r) = &job.resize {
        img = resize(&img, r).map_err(ConvertError::InvalidParams)?;
    }
    // 4. Поворот.
    if let Some(op) = job.rotate {
        img = rotate(&img, op);
    }
    // 5. Цветокоррекция.
    if let Some(adj) = &job.adjustments {
        if adj.is_active() {
            img = apply_adjustments(&img, adj);
        }
    }
    // 6. Удаление фона.
    if let Some(bg) = &job.remove_background {
        if !job.target_format.supports_alpha() {
            return Err(ConvertError::InvalidParams(
                "удаление фона требует формат с прозрачностью (PNG/WebP/GIF/TIFF)".into(),
            ));
        }
        img = remove_background_solid(&img, bg);
    }

    // 7. Путь вывода.
    let output_path = build_output_path(
        &job.source,
        job.output_dir.as_deref(),
        Some(job.target_format.extension()),
        job.suffix.as_deref(),
        job.overwrite,
    )?;

    // 8. Кодирование.
    encode_to_file(&img, job.target_format, &job.encode, &output_path)?;

    // 9. DPI записывается пост-фактум (крейт image не пишет pHYs/JFIF).
    if let Some((dx, dy)) = job.encode.dpi {
        match job.target_format {
            ImageOutputFormat::Jpeg => jpeg_set_dpi(&output_path, (dx, dy))?,
            ImageOutputFormat::Png => png_set_dpi(&output_path, (dx, dy))?,
            _ => { /* для остальных форматов DPI не поддержан движком */ }
        }
    }

    // 10. Перенос EXIF (только JPEG→JPEG через повторную вставку APP1).
    if job.keep_exif && matches!(job.target_format, ImageOutputFormat::Jpeg) {
        transfer_exif_jpeg(&job.source, &output_path)?;
    }

    let output_size = std::fs::metadata(&output_path)?.len();
    Ok(ImageJobResult {
        output_path,
        input_size,
        output_size,
        size_change: if input_size > 0 {
            output_size as f32 / input_size as f32 - 1.0
        } else {
            0.0
        },
        width: img.width(),
        height: img.height(),
        duration_ms: started.elapsed().as_millis(),
    })
}

/// Переносит EXIF-сегмент APP1 из одного JPEG в другой.
fn transfer_exif_jpeg(from: &Path, to: &Path) -> ConvertResult<()> {
    let src = std::fs::read(from)?;
    let app1 = find_jpeg_app1(&src)
        .ok_or_else(|| ConvertError::Processing("исходник без EXIF".into()))?;
    let dst = std::fs::read(to)?;
    let mut out = Vec::with_capacity(dst.len() + app1.len());
    out.extend_from_slice(&dst[..2]);
    out.extend_from_slice(&app1);
    out.extend_from_slice(&dst[2..]);
    std::fs::write(to, &out)?;
    Ok(())
}

fn find_jpeg_app1(data: &[u8]) -> Option<Vec<u8>> {
    let mut pos = 2;
    while pos + 4 <= data.len() {
        if data[pos] != 0xFF {
            return None;
        }
        let marker = data[pos + 1];
        let len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        if marker == 0xE1 && pos + 2 + len <= data.len() {
            if data[pos + 4..].starts_with(b"Exif") {
                return Some(data[pos..pos + 2 + len].to_vec());
            }
        }
        if marker == 0xDA {
            break;
        }
        pos += 2 + len;
    }
    None
}

/// Собирает ImageSourceInfo (используется Smart Scan и File Compare).
pub fn image_source_info(path: &Path) -> ConvertResult<ImageSourceInfo> {
    let meta = read_image_metadata(path)?;
    Ok(ImageSourceInfo {
        path: path.to_path_buf(),
        width: meta.width,
        height: meta.height,
        format: meta.format,
        file_size: meta.file_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, ImageBuffer, Rgba};

    fn make_png(dir: &Path, name: &str, w: u32, h: u32) -> std::path::PathBuf {
        let mut img = ImageBuffer::new(w, h);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 200 / w.max(1)) as u8, (y * 200 / h.max(1)) as u8, 90, 255]);
        }
        let path = dir.join(name);
        img.save(&path).unwrap();
        path
    }

    #[test]
    fn full_pipeline_png_to_webp_with_resize() {
        let dir = std::env::temp_dir().join("convert_image_job1");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = make_png(&dir, "in.png", 100, 60);

        let job = ImageJob {
            source: src.clone(),
            target_format: ImageOutputFormat::WebP { lossless: true },
            resize: Some(ResizeParams { width: 50, height: 30, mode: ResizeMode2::Exact, high_quality: true }),
            encode: EncodeParams { quality: 80, dpi: None },
            ..Default::default()
        };
        let result = run_job(&job).unwrap();
        assert!(result.output_path.exists());
        assert_eq!(result.output_path.extension().unwrap(), "webp");
        assert_eq!(result.width, 50);
        assert_eq!(result.height, 30);
        assert!(result.input_size > 0);
        let decoded = image::open(&result.output_path).unwrap();
        assert_eq!(decoded.dimensions(), (50, 30));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn jpeg_to_png_with_crop_and_rotate() {
        let dir = std::env::temp_dir().join("convert_image_job2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let img = image::RgbImage::from_fn(80, 40, |x, y| image::Rgb([(x * 3) as u8, (y * 5) as u8, 200]));
        let src = dir.join("in.jpg");
        image::save_buffer_with_format(
            &src,
            img.as_raw(),
            80,
            40,
            image::ExtendedColorType::Rgb8,
            image::ImageFormat::Jpeg,
        )
        .unwrap();

        let job = ImageJob {
            source: src.clone(),
            target_format: ImageOutputFormat::Png,
            crop: Some(CropParams { x: 10, y: 10, width: 40, height: 20 }),
            rotate: Some(RotationOp::Rotate90),
            ..Default::default()
        };
        let result = run_job(&job).unwrap();
        // 40x20 → поворот 90 → 20x40.
        assert_eq!((result.width, result.height), (20, 40));
        assert_eq!(result.output_path.extension().unwrap(), "png");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn background_removal_pipeline() {
        let dir = std::env::temp_dir().join("convert_image_job3");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut img = ImageBuffer::new(40, 40);
        for (x, y, px) in img.enumerate_pixels_mut() {
            let inside = (10..30).contains(&x) && (10..30).contains(&y);
            *px = if inside { Rgba([200, 30, 30, 255]) } else { Rgba([255, 255, 255, 255]) };
        }
        let src = dir.join("in.png");
        DynamicImage::ImageRgba8(img).save(&src).unwrap();

        let job = ImageJob {
            source: src,
            target_format: ImageOutputFormat::Png,
            remove_background: Some(BackgroundRemovalParams::default()),
            ..Default::default()
        };
        let result = run_job(&job).unwrap();
        let out = image::open(&result.output_path).unwrap();
        let rgba = out.to_rgba8();
        assert_eq!(rgba.get_pixel(20, 20)[3], 255);
        assert_eq!(rgba.get_pixel(1, 1)[3], 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dpi_written_to_result() {
        let dir = std::env::temp_dir().join("convert_image_job4");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = make_png(&dir, "in.png", 30, 30);
        let job = ImageJob {
            source: src,
            target_format: ImageOutputFormat::Jpeg,
            encode: EncodeParams { quality: 90, dpi: Some((300, 300)) },
            ..Default::default()
        };
        let result = run_job(&job).unwrap();
        assert_eq!(read_dpi(&result.output_path), Some((300, 300)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn background_removal_rejects_jpeg() {
        let dir = std::env::temp_dir().join("convert_image_job5");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = make_png(&dir, "in.png", 10, 10);
        let job = ImageJob {
            source: src,
            target_format: ImageOutputFormat::Jpeg,
            remove_background: Some(BackgroundRemovalParams::default()),
            ..Default::default()
        };
        assert!(run_job(&job).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Локальный алиас, чтобы не тянуть ResizeMode в тестах.
    type ResizeMode2 = crate::ops::ResizeMode;
}

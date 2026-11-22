//! Кодирование изображений с настройками качества и метаданными.

use crate::metadata::png_set_dpi;
use convert_core::error::{ConvertError, ConvertResult};
use image::{codecs, DynamicImage, ImageEncoder};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// Целевые форматы движка изображений.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageOutputFormat {
    Jpeg,
    Png,
    Gif,
    Bmp,
    WebP { lossless: bool },
    Tiff,
    Ico,
}

impl ImageOutputFormat {
    /// Расширение файла результата.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Gif => "gif",
            Self::Bmp => "bmp",
            Self::WebP { .. } => "webp",
            Self::Tiff => "tiff",
            Self::Ico => "ico",
        }
    }

    /// Поддерживает ли формат прозрачность.
    pub fn supports_alpha(self) -> bool {
        matches!(self, Self::Png | Self::Gif | Self::WebP { .. } | Self::Tiff | Self::Ico)
    }

    /// Есть ли потеря качества.
    pub fn is_lossy(self) -> bool {
        matches!(self, Self::Jpeg | Self::WebP { lossless: false })
    }
}

/// Параметры кодирования.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodeParams {
    /// Качество JPEG/WebP: 1–100 (по умолчанию 90).
    pub quality: u8,
    /// Разрешение для печати: DPI записывается в файл (PNG/JPEG).
    pub dpi: Option<(u16, u16)>,
}

impl Default for EncodeParams {
    fn default() -> Self {
        Self { quality: 90, dpi: None }
    }
}

/// Сохраняет изображение в файл с заданными параметрами.
pub fn encode_to_file(
    img: &DynamicImage,
    format: ImageOutputFormat,
    params: &EncodeParams,
    path: &Path,
) -> ConvertResult<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    encode_to_writer(img, format, params, &mut writer)
}

/// Кодирует изображение в произвольный writer (для тестов и предпросмотра).
/// TIFF требует Seek, поэтому ему передаётся Cursor над вектором.
pub fn encode_to_writer<W: std::io::Write>(
    img: &DynamicImage,
    format: ImageOutputFormat,
    params: &EncodeParams,
    writer: &mut W,
) -> ConvertResult<()> {
    let quality = params.quality.clamp(1, 100);

    // JPEG и BMP не хранят альфа-канал: смешиваем с белым фоном.
    let prepared: DynamicImage;
    let img_ref: &DynamicImage = if format.supports_alpha() {
        img
    } else {
        prepared = flatten_alpha(img, [255, 255, 255]);
        &prepared
    };

    match format {
        ImageOutputFormat::Jpeg => {
            let rgb = img_ref.to_rgb8();
            let encoder = codecs::jpeg::JpegEncoder::new_with_quality(writer, quality);
            encoder
                .write_image(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)
                .map_err(|e| ConvertError::Processing(format!("JPEG: {e}")))?;
        }
        ImageOutputFormat::Png => {
            let rgba = img_ref.to_rgba8();
            let mut png = codecs::png::PngEncoder::new(writer);
            png.write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| ConvertError::Processing(format!("PNG: {e}")))?;
        }
        ImageOutputFormat::Gif => {
            let rgba = img_ref.to_rgba8();
            let mut gif = codecs::gif::GifEncoder::new(writer);
            gif.encode_frame(image::Frame::new(rgba))
                .map_err(|e| ConvertError::Processing(format!("GIF: {e}")))?;
        }
        ImageOutputFormat::Bmp => {
            let rgb = img_ref.to_rgb8();
            let mut bmp = codecs::bmp::BmpEncoder::new(writer);
            bmp.write_image(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)
                .map_err(|e| ConvertError::Processing(format!("BMP: {e}")))?;
        }
        ImageOutputFormat::WebP { lossless } => {
            // Крейт image поддерживает только lossless WebP (VP8L).
            // Lossy-WebP выполняет FFmpeg-движок (convert-media);
            // сюда lossy попадать не должен — проверим и честно откажем.
            if !lossless {
                return Err(ConvertError::InvalidParams(
                    "lossy WebP обрабатывает FFmpeg-движок (convert-media)".into(),
                ));
            }
            let rgba = img_ref.to_rgba8();
            let encoder = codecs::webp::WebPEncoder::new_lossless(writer);
            encoder
                .write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8)
                .map_err(|e| ConvertError::Processing(format!("WebP: {e}")))?;
        }
        ImageOutputFormat::Tiff => {
            let rgba = img_ref.to_rgba8();
            // TiffEncoder требует Seek: пишем в память, затем копируем в writer.
            let mut buf = std::io::Cursor::new(Vec::new());
            let mut tiff = codecs::tiff::TiffEncoder::new(&mut buf);
            tiff.write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|e| ConvertError::Processing(format!("TIFF: {e}")))?;
            writer.write_all(buf.get_ref())?;
        }
        ImageOutputFormat::Ico => {
            let rgba = img_ref.to_rgba8();
            let mut ico = codecs::ico::IcoEncoder::new(writer);
            ico.write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8)
                .map_err(|e| ConvertError::Processing(format!("ICO: {e}")))?;
        }
    }
    Ok(())
}

/// Замешивает альфа-канал на однотонный фон (для JPEG/BMP).
pub fn flatten_alpha(img: &DynamicImage, background: [u8; 3]) -> DynamicImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut out = image::ImageBuffer::new(w, h);
    for (x, y, px) in rgba.enumerate_pixels() {
        let a = px[3] as f32 / 255.0;
        let mix = |c: u8, bg: u8| (c as f32 * a + bg as f32 * (1.0 - a)).round() as u8;
        out.put_pixel(x, y, image::Rgba([
            mix(px[0], background[0]),
            mix(px[1], background[1]),
            mix(px[2], background[2]),
            255,
        ]));
    }
    DynamicImage::ImageRgba8(out)
}

/// Оценка размера файла после кодирования (для предпросмотра настроек):
/// кодирует в память и возвращает длину.
pub fn estimate_encoded_size(
    img: &DynamicImage,
    format: ImageOutputFormat,
    params: &EncodeParams,
) -> ConvertResult<u64> {
    let mut buf = Vec::new();
    encode_to_writer(img, format, params, &mut buf)?;
    Ok(buf.len() as u64)
}

/// Вспомогательная функция для job: пишет PNG-DPI повторным сохранением
/// (крейт image не умеет pHYs при первом проходе).
pub fn apply_png_dpi(path: &Path, dpi: (u16, u16)) -> ConvertResult<()> {
    png_set_dpi(path, dpi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, ImageBuffer, Rgba};

    fn gradient(w: u32, h: u32) -> DynamicImage {
        let mut img = ImageBuffer::new(w, h);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 128, 255]);
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn encodes_all_formats() {
        let img = gradient(32, 32);
        let dir = std::env::temp_dir().join("convert_image_encode");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        for fmt in [
            ImageOutputFormat::Jpeg,
            ImageOutputFormat::Png,
            ImageOutputFormat::Gif,
            ImageOutputFormat::Bmp,
            ImageOutputFormat::WebP { lossless: true },
            ImageOutputFormat::Tiff,
        ] {
            let path = dir.join(format!("out.{}", fmt.extension()));
            encode_to_file(&img, fmt, &EncodeParams::default(), &path).unwrap();
            let meta = std::fs::metadata(&path).unwrap();
            assert!(meta.len() > 0, "пустой файл для {fmt:?}");
            // Результат должен открываться движком как изображение.
            let reloaded = image::open(&path).unwrap();
            assert_eq!(reloaded.dimensions(), (32, 32));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn jpeg_quality_changes_size() {
        let img = gradient(256, 256);
        let big = estimate_encoded_size(&img, ImageOutputFormat::Jpeg, &EncodeParams { quality: 98, dpi: None }).unwrap();
        let small = estimate_encoded_size(&img, ImageOutputFormat::Jpeg, &EncodeParams { quality: 30, dpi: None }).unwrap();
        assert!(big > small, "качество 98 ({big} байт) должно весить больше 30 ({small} байт)");
    }

    #[test]
    fn alpha_flattened_for_jpeg() {
        let mut img = ImageBuffer::new(8, 8);
        for (x, y, px) in img.enumerate_pixels_mut() {
            let alpha = if x < 4 { 255 } else { 0 };
            *px = Rgba([255, 0, 0, alpha]);
        }
        let img = DynamicImage::ImageRgba8(img);
        let mut buf = Vec::new();
        encode_to_writer(&img, ImageOutputFormat::Jpeg, &EncodeParams::default(), &mut buf).unwrap();
        let decoded = image::load_from_memory(&buf).unwrap();
        let rgba = decoded.to_rgba8();
        let left = rgba.get_pixel(1, 4);
        let right = rgba.get_pixel(6, 4);
        // Слева красный, справа — белый фон (с допуском на JPEG-шум).
        assert!(left[0] > 200 && left[1] < 60, "left = {left:?}");
        assert!(right[0] > 200 && right[1] > 200, "right = {right:?}");
    }

    #[test]
    fn format_flags() {
        assert!(ImageOutputFormat::Png.supports_alpha());
        assert!(!ImageOutputFormat::Jpeg.supports_alpha());
        assert!(ImageOutputFormat::Jpeg.is_lossy());
        assert!(!ImageOutputFormat::WebP { lossless: true }.is_lossy());
    }
}

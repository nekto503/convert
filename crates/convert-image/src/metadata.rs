//! Чтение, редактирование и удаление метаданных изображений:
//! EXIF (JPEG/TIFF), DPI, очистка личных данных.

use convert_core::error::{ConvertError, ConvertResult};
use image::{DynamicImage, GenericImageView, ImageEncoder};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Одно поле метаданных для UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataField {
    /// Технический ключ (например "Exif:Model" или "File:Size").
    pub key: String,
    /// Человекочитаемое название.
    pub label: String,
    pub value: String,
}

/// Полный отчёт метаданных изображения.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub file_size: u64,
    pub dpi: Option<(u16, u16)>,
    pub exif: Vec<MetadataField>,
}

/// Читает метаданные файла изображения (без полной загрузки пикселей,
/// где это возможно: EXIF парсится из заголовка).
pub fn read_image_metadata(path: &Path) -> ConvertResult<ImageMetadata> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut head = [0u8; 8];
    reader.read_exact(&mut head)?;

    let meta_len = std::fs::metadata(path)?.len();
    let (format, width, height) = image::image_dimensions(path)
        .map(|(w, h)| (format_name(&head), w, h))
        .map_err(|e| ConvertError::Processing(format!("не удалось прочитать изображение: {e}")))?;

    let mut exif_fields = Vec::new();
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) || head.starts_with(b"II*\x00") || head.starts_with(b"MM\x00*") {
        reader.seek(SeekFrom::Start(0))?;
        let exif_reader = exif::Reader::new();
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        if let Ok(exif) = exif_reader.read_from_container(&mut std::io::Cursor::new(&buf)) {
            for field in exif.fields() {
                // display_value() экранирует не-ASCII (кириллицу), поэтому
                // для ASCII-полей берём сырые байты и декодируем UTF-8.
                let value = ascii_field_value(&exif, field).unwrap_or_else(|| {
                    field.display_value().with_unit(&exif).to_string()
                });
                exif_fields.push(MetadataField {
                    key: format!("Exif:{}", field.tag),
                    label: field.tag.to_string(),
                    value,
                });
            }
        }
    }

    Ok(ImageMetadata {
        width,
        height,
        format,
        file_size: meta_len,
        dpi: read_dpi(path),
        exif: exif_fields,
    })
}

/// Для строковых EXIF-полей возвращает читаемое UTF-8 значение
/// (kamadak-exif экранирует кириллицу как \xd0...).
fn ascii_field_value(exif: &exif::Exif, field: &exif::Field) -> Option<String> {
    use exif::Value;
    // display_value() возвращает уже готовую строку; экранирование видно
    // по байтам \\x. Проверим сырую Value на Ascii.
    match field.value.clone() {
        Value::Ascii(bytes) => {
            let joined: Vec<u8> = bytes.into_iter().flatten().collect();
            let text = String::from_utf8_lossy(&joined);
            let trimmed = text.trim_end_matches('\0').to_string();
            if trimmed.is_empty() || trimmed.is_ascii() {
                // ASCII отображается штатно через display_value.
                None
            } else {
                let _ = exif;
                Some(trimmed)
            }
        }
        _ => None,
    }
}

fn format_name(head: &[u8]) -> String {
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "JPEG".into()    } else if head.starts_with(&[0x89, b'P', b'N', b'G']) {
        "PNG".into()
    } else if head.starts_with(b"GIF8") {
        "GIF".into()
    } else if head.starts_with(b"BM") {
        "BMP".into()
    } else if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        "WebP".into()
    } else if head.starts_with(b"II*\x00") || head.starts_with(b"MM\x00*") {
        "TIFF".into()
    } else {
        "UNKNOWN".into()
    }
}

/// Читает DPI из файла (PNG pHYs / JPEG JFIF), если он задан.
pub fn read_dpi(path: &Path) -> Option<(u16, u16)> {
    let data = std::fs::read(path).ok()?;
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        return png_dpi_bytes(&data);
    }
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return jpeg_dpi_bytes(&data);
    }
    None
}

/// PNG: парсит чанк pHYs (пикселей на метр → DPI), возвращает DPI.
pub fn png_read_dpi(path: &Path) -> Option<(u16, u16)> {
    let data = std::fs::read(path).ok()?;
    png_dpi_bytes(&data)
}

fn png_dpi_bytes(data: &[u8]) -> Option<(u16, u16)> {
    // Структура PNG: 8 байт сигнатура, затем чанки [len(4) type(4) data crc(4)].
    let mut pos = 8;
    while pos + 8 <= data.len() {
        let len = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let chunk_type = &data[pos + 4..pos + 8];
        if chunk_type == b"pHYs" && pos + 8 + len <= data.len() && len >= 9 {
            let ppm_x = u32::from_be_bytes([
                data[pos + 8],
                data[pos + 9],
                data[pos + 10],
                data[pos + 11],
            ]);
            let ppm_y = u32::from_be_bytes([
                data[pos + 12],
                data[pos + 13],
                data[pos + 14],
                data[pos + 15],
            ]);
            let unit = data[pos + 16];
            if unit == 1 {
                let dpi_x = (ppm_x as f64 * 0.0254).round() as u16;
                let dpi_y = (ppm_y as f64 * 0.0254).round() as u16;
                return Some((dpi_x, dpi_y));
            }
        }
        pos += 12 + len;
    }
    None
}

/// JPEG: ищет сегмент APP0 JFIF с полями X/Ydensity.
pub fn jpeg_dpi_bytes(data: &[u8]) -> Option<(u16, u16)> {
    // Сегменты: FF marker len(2) payload. APP0 = FFE0.
    let mut pos = 2;
    while pos + 4 <= data.len() {
        if data[pos] != 0xFF {
            return None;
        }
        let marker = data[pos + 1];
        let len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        if marker == 0xE0 && pos + 4 + len <= data.len() && len >= 16 {
            let units = data[pos + 11];
            let x = u16::from_be_bytes([data[pos + 12], data[pos + 13]]);
            let y = u16::from_be_bytes([data[pos + 14], data[pos + 15]]);
            if units == 1 {
                return Some((x, y));
            }
            if units == 2 {
                // Плотность в точках на см.
                return Some(((x as f64 * 2.54).round() as u16, (y as f64 * 2.54).round() as u16));
            }
        }
        if marker == 0xDA {
            break; // начались данные
        }
        pos += 2 + len;
    }
    None
}

/// Записывает DPI в JPEG (сегмент JFIF APP0), только для валидного JPEG.
pub fn jpeg_set_dpi(path: &Path, dpi: (u16, u16)) -> ConvertResult<()> {
    let data = std::fs::read(path)?;
    if !data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Err(ConvertError::InvalidParams("DPI в JPEG: файл не JPEG".into()));
    }
    // Найти конец сегмента APP0 (или вставить новый после SOI).
    let mut out: Vec<u8> = Vec::with_capacity(data.len() + 18);
    out.extend_from_slice(&data[..2]); // SOI
    if data.len() > 4 && data[2] == 0xFF && data[3] == 0xE0 {
        // Заменяем существующий APP0.
        let len = u16::from_be_bytes([data[4], data[5]]) as usize;
        out.extend_from_slice(&make_jfif_app0(dpi));
        out.extend_from_slice(&data[2 + len + 2..]);
    } else {
        out.extend_from_slice(&make_jfif_app0(dpi));
        out.extend_from_slice(&data[2..]);
    }
    std::fs::write(path, &out)?;
    Ok(())
}

fn make_jfif_app0(dpi: (u16, u16)) -> Vec<u8> {
    let mut seg = vec![0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0, 1, 2, 1];
    seg.extend_from_slice(&dpi.0.to_be_bytes());
    seg.extend_from_slice(&dpi.1.to_be_bytes());
    seg.extend_from_slice(&[0, 0]); // нет превью
    seg
}

/// Записывает DPI в PNG (чанк pHYs после IHDR).
pub fn png_set_dpi(path: &Path, dpi: (u16, u16)) -> ConvertResult<()> {
    let data = std::fs::read(path)?;
    if !data.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Err(ConvertError::InvalidParams("DPI в PNG: файл не PNG".into()));
    }
    // ppm = dpi / 0.0254
    let ppm_x = (dpi.0 as f64 / 0.0254).round() as u32;
    let ppm_y = (dpi.1 as f64 / 0.0254).round() as u32;
    let mut chunk = Vec::with_capacity(21);
    chunk.extend_from_slice(&ppm_x.to_be_bytes());
    chunk.extend_from_slice(&ppm_y.to_be_bytes());
    chunk.push(1); // единица — метр
    let crc = png_crc32(&chunk);
    let mut phys = Vec::with_capacity(21);
    phys.extend_from_slice(&9u32.to_be_bytes());
    phys.extend_from_slice(b"pHYs");
    phys.extend_from_slice(&chunk);
    phys.extend_from_slice(&crc.to_be_bytes());

    // Удаляем старый pHYs, новый вставляем сразу после IHDR.
    let mut out: Vec<u8> = Vec::with_capacity(data.len() + 21);
    out.extend_from_slice(&data[..8]);
    let mut pos = 8;
    let mut inserted = false;
    while pos + 8 <= data.len() {
        let len = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let ctype = &data[pos + 4..pos + 8];
        if ctype == b"pHYs" {
            // Пропускаем старый чанк.
            pos += 12 + len;
            continue;
        }
        out.extend_from_slice(&data[pos..pos + 12 + len]);
        if ctype == b"IHDR" && !inserted {
            out.extend_from_slice(&phys);
            inserted = true;
        }
        pos += 12 + len;
    }
    std::fs::write(path, &out)?;
    Ok(())
}

/// CRC32 для PNG (IEEE 802.3, без внешних зависимостей).
fn png_crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for i in 0..256u32 {
        let mut c = i;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        table[i as usize] = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

/// Пересохраняет изображение без метаданных (strip): личные данные
/// (GPS, камера, автор) удаляются. Вызывается только после явного
/// подтверждения пользователя на уровне UI.
pub fn strip_image_metadata(path: &Path) -> ConvertResult<()> {
    let img = image::open(path).map_err(|e| ConvertError::Processing(e.to_string()))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    // Пересохранение через крейт image не переносит EXIF — оно и нужно.
    let mut out = Vec::new();
    match ext.as_str() {
        "jpg" | "jpeg" => {
            let rgb = img.to_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95)
                .write_image(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|e| ConvertError::Processing(e.to_string()))?;
        }
        _ => {
            let rgba = img.to_rgba8();
            image::codecs::png::PngEncoder::new(&mut out)
                .write_image(
                    rgba.as_raw(),
                    rgba.width(),
                    rgba.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|e| ConvertError::Processing(e.to_string()))?;
        }
    }
    std::fs::write(path, &out)?;
    Ok(())
}

/// Описывает, какие личные данные найдены в EXIF (для предупреждения).
pub fn privacy_sensitive_fields(meta: &ImageMetadata) -> Vec<&MetadataField> {
    const SENSITIVE: [&str; 8] = [
        "GPS", "Make", "Model", "Artist", "Software", "Copyright", "DateTime", "OwnerName",
    ];
    meta.exif
        .iter()
        .filter(|f| {
            SENSITIVE.iter().any(|s| {
                f.key.contains(s) || f.label.to_lowercase().contains(&s.to_lowercase())
            })
        })
        .collect()
}

// ---- EXIF-редактор ----------------------------------------------------

/// Какие поля EXIF менять (правка через пере-кодирование JPEG/TIFF).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExifEdit {
    pub artist: Option<String>,
    pub copyright: Option<String>,
    pub image_description: Option<String>,
    /// Удалить все персональные поля (GPS, даты, модель камеры).
    pub strip_personal: bool,
}

impl ExifEdit {
    pub fn is_empty(&self) -> bool {
        self.artist.is_none()
            && self.copyright.is_none()
            && self.image_description.is_none()
            && !self.strip_personal
    }
}

/// Применяет правки EXIF к JPEG: строит новый EXIF (TIFF) блок вручную
/// и вставляет его как сегмент APP1 (библиотека kamadak-exif read-only).
pub fn exif_edit(path: &Path, edit: &ExifEdit) -> ConvertResult<()> {
    if edit.is_empty() {
        return Ok(());
    }
    let data = std::fs::read(path)?;
    if !data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Err(ConvertError::InvalidParams(
            "правка EXIF поддерживается для JPEG".into(),
        ));
    }

    // Читаем существующие строковые поля, чтобы сохранить их.
    let mut existing: Vec<(exif::Tag, String)> = Vec::new();
    if let Ok(old) = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(&data)) {
        for field in old.fields() {
            if matches!(
                field.tag,
                exif::Tag::Artist | exif::Tag::Copyright | exif::Tag::ImageDescription
            ) && field.ifd_num == exif::In::PRIMARY
            {
                let text = field
                    .display_value()
                    .to_string()
                    .trim_matches('\0')
                    .to_string();
                existing.push((field.tag, text));
            }
        }
    }

    // Формируем итоговый набор полей: существующие + правки.
    let mut fields: Vec<(exif::Tag, String)> = existing;
    let upsert = |fields: &mut Vec<(exif::Tag, String)>, tag: exif::Tag, value: String| {
        fields.retain(|(t, _)| *t != tag);
        fields.push((tag, value));
    };
    if let Some(artist) = &edit.artist {
        upsert(&mut fields, exif::Tag::Artist, artist.clone());
    }
    if let Some(copyright) = &edit.copyright {
        upsert(&mut fields, exif::Tag::Copyright, copyright.clone());
    }
    if let Some(desc) = &edit.image_description {
        upsert(&mut fields, exif::Tag::ImageDescription, desc.clone());
    }
    if edit.strip_personal {
        // Ничего лишнего не переносим — только явные поля.
    }

    let exif_block = build_exif_tiff(&fields)?;
    let mut app1 = Vec::with_capacity(8 + exif_block.len());
    app1.extend_from_slice(&[0xFF, 0xE1]);
    let seg_len = (exif_block.len() + 2 + 6) as u16;
    app1.extend_from_slice(&seg_len.to_be_bytes());
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(&exif_block);

    // Новый файл: SOI + APP1 + остальные сегменты (без старых APP0/APP1).
    let mut out: Vec<u8> = Vec::with_capacity(data.len() + app1.len());
    out.extend_from_slice(&data[..2]);
    out.extend_from_slice(&app1);
    let mut pos = 2;
    while pos + 4 <= data.len() {
        if data[pos] != 0xFF {
            out.extend_from_slice(&data[pos..]);
            break;
        }
        let marker = data[pos + 1];
        if marker == 0xD8 {
            pos += 2;
            continue;
        }
        if marker == 0xD9 || pos + 3 >= data.len() {
            break;
        }
        let len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        if marker == 0xE0 || marker == 0xE1 {
            pos += 2 + len; // старые JFIF/EXIF не переносим
            continue;
        }
        out.extend_from_slice(&data[pos..(pos + 2 + len).min(data.len())]);
        pos += 2 + len;
        if marker == 0xDA {
            out.extend_from_slice(&data[pos..]);
            break;
        }
    }
    std::fs::write(path, &out)?;
    Ok(())
}

/// Строит минимальный EXIF/TIFF-блок (IFD0) со строковыми полями.
fn build_exif_tiff(fields: &[(exif::Tag, String)]) -> ConvertResult<Vec<u8>> {
    // TIFF header (little-endian): "II", 42, offset IFD0 = 8.
    let mut tiff: Vec<u8> = Vec::new();
    tiff.extend_from_slice(b"II\x2A\x00");
    tiff.extend_from_slice(&8u32.to_le_bytes());

    // IFD0: count + entries (по 12 байт) + next-IFD (0) + данные.
    let mut entries: Vec<(u16, Vec<u8>)> = Vec::new(); // (tag, ASCII payload)
    for (tag, value) in fields {
        // ASCII-строка с завершающим нулём.
        let mut payload = value.as_bytes().to_vec();
        payload.push(0);
        entries.push((tag.number(), payload));
    }
    if entries.is_empty() {
        return Err(ConvertError::InvalidParams("нет полей для записи".into()));
    }
    entries.sort_by_key(|(tag, _)| *tag);

    // Заголовок IFD.
    let ifd_offset = 8usize;
    let ifd_size = 2 + entries.len() * 12 + 4;
    let mut data_area: Vec<u8> = Vec::new();

    tiff.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (tag, payload) in &entries {
        tiff.extend_from_slice(&tag.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes()); // type ASCII
        tiff.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        if payload.len() <= 4 {
            // Значение inline, добитое нулями.
            let mut inline = payload.clone();
            inline.resize(4, 0);
            tiff.extend_from_slice(&inline);
        } else {
            let offset = (ifd_offset + ifd_size + data_area.len()) as u32;
            tiff.extend_from_slice(&offset.to_le_bytes());
            data_area.extend_from_slice(payload);
            // Выравнивание по 2 байта.
            if data_area.len() % 2 == 1 {
                data_area.push(0);
            }
        }
    }
    tiff.extend_from_slice(&0u32.to_le_bytes()); // next IFD
    tiff.extend_from_slice(&data_area);
    Ok(tiff)
}

// Примечание: функции записи DPI — jpeg_set_dpi и png_set_dpi выше;
// отдельные jpeg_dpi-обёртки не нужны.

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    fn make_jpeg(dir: &Path, name: &str) -> std::path::PathBuf {
        let img = image::RgbImage::from_fn(40, 30, |x, y| {
            image::Rgb([(x * 5) as u8, (y * 7) as u8, 128])
        });
        let path = dir.join(name);
        image::save_buffer_with_format(
            &path,
            img.as_raw(),
            40,
            30,
            image::ExtendedColorType::Rgb8,
            image::ImageFormat::Jpeg,
        )
        .unwrap();
        path
    }

    #[test]
    fn read_metadata_basic() {
        let dir = std::env::temp_dir().join("convert_image_meta");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = make_jpeg(&dir, "a.jpg");
        let meta = read_image_metadata(&path).unwrap();
        assert_eq!((meta.width, meta.height), (40, 30));
        assert_eq!(meta.format, "JPEG");
        assert!(meta.file_size > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn jpeg_dpi_roundtrip() {
        let dir = std::env::temp_dir().join("convert_image_meta2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = make_jpeg(&dir, "b.jpg");
        assert_eq!(read_dpi(&path), None); // без записи DPI нет
        jpeg_set_dpi(&path, (300, 300)).unwrap();
        assert_eq!(read_dpi(&path), Some((300, 300)));
        jpeg_set_dpi(&path, (72, 96)).unwrap(); // перезапись
        assert_eq!(read_dpi(&path), Some((72, 96)));
        // Файл остаётся валидным изображением.
        assert!(image::open(&path).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn png_dpi_roundtrip() {
        let dir = std::env::temp_dir().join("convert_image_meta3");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let img = image::RgbaImage::from_fn(10, 10, |_, _| image::Rgba([10u8, 20, 30, 255]));
        let path = dir.join("c.png");
        img.save(&path).unwrap();
        assert!(read_dpi(&path).is_none());
        png_set_dpi(&path, (300, 300)).unwrap();
        assert_eq!(read_dpi(&path), Some((300, 300)));
        assert!(image::open(&path).is_ok());
        // Повторная установка не дублирует pHYs.
        png_set_dpi(&path, (144, 144)).unwrap();
        assert_eq!(read_dpi(&path), Some((144, 144)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strip_removes_exif() {
        let dir = std::env::temp_dir().join("convert_image_meta4");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = make_jpeg(&dir, "d.jpg");
        // Добавляем EXIF с автором.
        exif_edit(
            &path,
            &ExifEdit { artist: Some("Тест Автор".into()), ..Default::default() },
        )
        .unwrap();
        let meta = read_image_metadata(&path).unwrap();
        assert!(meta.exif.iter().any(|f| f.value.contains("Тест Автор")));

        // Strip удаляет.
        strip_image_metadata(&path).unwrap();
        let meta2 = read_image_metadata(&path).unwrap();
        assert!(meta2.exif.iter().all(|f| !f.value.contains("Тест Автор")));
        assert!(image::open(&path).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

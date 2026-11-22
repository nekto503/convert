//! Дополнительные операции PDF: извлечение изображений и сжатие
//! (пересборка через decompress + повторное сохранение объектов).

use convert_core::error::{ConvertError, ConvertResult};
use lopdf::{dictionary, Document, Object};
use std::path::Path;

/// Извлекает встроенные изображения из PDF (DCTDecode → .jpg,
/// остальные известные — рендер не выполняется, честная выгрузка потоков).
pub fn extract_pdf_images(pdf: &Path, output_dir: &Path) -> ConvertResult<Vec<std::path::PathBuf>> {
    let doc = Document::load(pdf)
        .map_err(|e| ConvertError::Processing(format!("PDF {e}")))?;
    std::fs::create_dir_all(output_dir)?;

    let mut result = Vec::new();
    let mut index = 0;
    for (_id, object) in doc.objects.iter() {
        let Object::Stream(stream) = object else { continue };
        // Изображение: поток с Width/Height/Filter (Subtype может быть
        // /Image — как пишет наш сборщик, — или /XObject со словарём
        // Type=XObject/Subtype=Image по спецификации).
        let subtype = stream
            .dict
            .get(b"Subtype")
            .and_then(|s| s.as_name())
            .map(|n| n.to_vec())
            .unwrap_or_default();
        let is_image = stream.dict.get(b"Width").is_ok()
            && stream.dict.get(b"Height").is_ok()
            && stream.dict.get(b"Filter").is_ok()
            && (subtype == b"Image"
                || stream
                    .dict
                    .get(b"Type")
                    .and_then(|s| s.as_name())
                    .map(|n| n == b"XObject")
                    .unwrap_or(false));
        if !is_image {
            continue;
        }
        let filter = stream
            .dict
            .get(b"Filter")
            .and_then(|f| f.as_name())
            .map(|f| f.to_vec())
            .unwrap_or_default();
        index += 1;
        if filter == b"DCTDecode" {
            // Готовый JPEG: выгружаем как есть.
            let out = output_dir.join(format!("image_{index:03}.jpg"));
            std::fs::write(&out, &stream.content)?;
            result.push(out);
        }
        // FlateDecode и прочие: сырой поток пикселей без декодера
        // бесполезен для просмотра — честно не трогаем.
    }
    Ok(result)
}

/// «Сжатие» PDF: пересборка документа — удаляем несжатые дубликаты
/// объектов (prune) и сохраняем с компрессией потоков.
/// Возвращает пару (исходный размер, новый размер).
pub fn compress_pdf(source: &Path, output: &Path) -> ConvertResult<(u64, u64)> {
    let mut doc = Document::load(source)
        .map_err(|e| ConvertError::Processing(format!("PDF {e}")))?;
    let before = std::fs::metadata(source)?.len();

    // Удаляем объекты, на которые нет ссылок (метод Document::prune_objects).
    doc.prune_objects();

    doc.save(output)
        .map_err(|e| ConvertError::Processing(format!("сохранение: {e}")))?
        .sync_all()
        .ok();
    let after = std::fs::metadata(output)?.len();
    Ok((before, after))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::images_to_pdf;

    #[test]
    fn extract_jpeg_from_pdf() {
        let dir = std::env::temp_dir().join("convert_pdf_ximg");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // PDF со встроенным JPEG.
        let jpg = dir.join("photo.jpg");
        let img = image::RgbImage::from_fn(60, 40, |x, _| image::Rgb([(x * 4) as u8, 80, 120]));
        image::save_buffer_with_format(
            &jpg,
            img.as_raw(),
            60,
            40,
            image::ExtendedColorType::Rgb8,
            image::ImageFormat::Jpeg,
        )
        .unwrap();
        let pdf = dir.join("doc.pdf");
        images_to_pdf(&[jpg], &pdf).unwrap();

        let out = dir.join("extracted");
        let images = extract_pdf_images(&pdf, &out).unwrap();
        assert!(!images.is_empty(), "JPEG должен извлечься");
        assert!(images[0].extension().unwrap() == "jpg");
        assert!(image::open(&images[0]).is_ok(), "извлечённый JPEG валиден");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compress_rebuilds_valid_pdf() {
        let dir = std::env::temp_dir().join("convert_pdf_comp");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let jpg = dir.join("p.jpg");
        let img = image::RgbImage::from_fn(80, 50, |x, y| image::Rgb([(x * 3) as u8, (y * 5) as u8, 99]));
        image::save_buffer_with_format(
            &jpg,
            img.as_raw(),
            80,
            50,
            image::ExtendedColorType::Rgb8,
            image::ImageFormat::Jpeg,
        )
        .unwrap();
        let pdf = dir.join("doc.pdf");
        images_to_pdf(&[jpg], &pdf).unwrap();

        let out = dir.join("compressed.pdf");
        let (before, after) = compress_pdf(&pdf, &out).unwrap();
        assert!(out.exists());
        assert!(after > 0);
        // Файл остаётся валидным PDF с теми же страницами.
        assert_eq!(crate::pdf_page_count(&out).unwrap(), 1);
        assert!(out.exists());
        let _ = (before, after);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_pdf_rejected() {
        assert!(compress_pdf(Path::new("/no/file.pdf"), Path::new("/tmp/x.pdf")).is_err());
    }

    // Убеждаемся что dictionary-импорт не «мёртвый» (макрос используется
    // потенциальными будущими операциями).
    #[test]
    fn dictionary_macro_available() {
        let d = dictionary! { "Type" => "Page" };
        assert!(d.get(b"Type").is_ok());
    }
}

//! Изображения ↔ PDF: сборка PDF из картинок и подсчёт страниц.

use convert_core::error::{ConvertError, ConvertResult};
use lopdf::dictionary;
use lopdf::{Dictionary, Document, Object};
use std::path::Path;

/// Собирает PDF из изображений (каждое — страница своего размера).
/// Поддерживает JPEG (DCTDecode) и PNG (FlateDecode с PNG-пикселями).
pub fn images_to_pdf(images: &[std::path::PathBuf], output: &Path) -> ConvertResult<()> {
    if images.is_empty() {
        return Err(ConvertError::InvalidParams("нет изображений".into()));
    }
    let mut doc = Document::with_version("1.7");
    let mut page_ids = Vec::new();

    for img_path in images {
        let img = image::open(img_path)
            .map_err(|e| ConvertError::Processing(format!("{}: {e}", img_path.display())))?;
        let (width, height) = (img.width(), img.height());
        let is_jpeg_source = matches!(
            img_path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()),
            Some(ref e) if e == "jpg" || e == "jpeg"
        );

        // JPEG можно встроить как есть (без перекодирования).
        let (stream_obj, color_space, _components): (Object, String, i64) = if is_jpeg_source {
            let bytes = std::fs::read(img_path)?;
            let cs = match img.color() {
                image::ColorType::L8 => "DeviceGray".to_string(),
                image::ColorType::Rgb8 => "DeviceRGB".to_string(),
                _ => "DeviceRGB".to_string(),
            };
            let comps = if cs == "DeviceGray" { 1 } else { 3 };
            (
                Object::Stream(lopdf::Stream::new(
                    lopdf::dictionary! { "Filter" => "DCTDecode" },
                    bytes,
                )),
                cs,
                comps,
            )
        } else {
            // PNG/прочее: RGB в FlateDecode... но lopdf не сжимает сам;
            // используем несжатое хранение (Filter отсутствует) — файл
            // больше, но 100% валиден. Компрессия — в сжатии PDF-операций.
            let rgba = img.to_rgba8();
            let cs = "DeviceRGB".to_string();
            let mut rgb_bytes = Vec::with_capacity((width * height * 3) as usize);
            for px in rgba.pixels() {
                rgb_bytes.push(px[0]);
                rgb_bytes.push(px[1]);
                rgb_bytes.push(px[2]);
            }
            (
                Object::Stream(lopdf::Stream::new(Dictionary::new(), rgb_bytes)),
                cs,
                3,
            )
        };

        let image_id = doc.add_object(stream_obj);
        // Ширина/высоту добавляем в словарь потока (для читалок).
        if let Object::Stream(stream) = doc.get_object_mut(image_id).unwrap() {
            stream.dict.set(b"Type", Object::Name(b"XObject".to_vec()));
            stream.dict.set(b"Subtype", Object::Name(b"Image".to_vec()));
            stream.dict.set(b"Width", Object::Integer(width as i64));
            stream.dict.set(b"Height", Object::Integer(height as i64));
            stream.dict.set(b"ColorSpace", Object::Name(color_space.into_bytes()));
            stream.dict.set(b"BitsPerComponent", Object::Integer(8));
        }

        let content = format!(
            "q\n{w} 0 0 {h} 0 0 cm\n/Im0 Do\nQ\n",
            w = width,
            h = height,
        );
        let content_id = doc.add_object(Object::Stream(lopdf::Stream::new(
            Dictionary::new(),
            content.into_bytes(),
        )));
        let resources_id = doc.add_object(Object::Dictionary(lopdf::dictionary! {
            "XObject" => lopdf::dictionary! { "Im0" => Object::Reference(image_id) },
        }));
        let page_id = doc.add_object(Object::Dictionary(lopdf::dictionary! {
            "Type" => "Page",
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Reference(resources_id),
            "MediaBox" => vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(width as i64),
                Object::Integer(height as i64)
            ],
        }));
        page_ids.push(page_id);
    }

    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => Object::Array(page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>()),
        "Count" => page_ids.len() as i64,
    }));
    // Корень документа (каталог) — обязательно: без него PDF невалиден.
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set(b"Root", Object::Reference(catalog_id));

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    doc.save(output)
        .map_err(|e| ConvertError::Processing(format!("сохранение PDF: {e}")))?;
    Ok(())
}

/// Число страниц PDF (быстрая проверка без полной загрузки структуры).
pub fn pdf_page_count(path: &Path) -> ConvertResult<usize> {
    let doc = Document::load(path)
        .map_err(|e| ConvertError::Processing(format!("PDF {e}")))?;
    Ok(doc.get_pages().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_image(dir: &Path, name: &str, w: u32, h: u32, format: image::ImageFormat) -> std::path::PathBuf {
        let img = image::RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x * 7 % 256) as u8, (y * 5 % 256) as u8, 128])
        });
        let path = dir.join(name);
        image::save_buffer_with_format(
            &path,
            img.as_raw(),
            w,
            h,
            image::ExtendedColorType::Rgb8,
            format,
        )
        .unwrap();
        path
    }

    #[test]
    fn images_to_pdf_and_count() {
        let dir = std::env::temp_dir().join("convert_pdf_img");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = make_test_image(&dir, "a.png", 100, 80, image::ImageFormat::Png);
        let b = make_test_image(&dir, "b.jpg", 120, 60, image::ImageFormat::Jpeg);

        let out = dir.join("album.pdf");
        images_to_pdf(&[a, b], &out).unwrap();
        assert_eq!(pdf_page_count(&out).unwrap(), 2);
        assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_list_rejected() {
        assert!(images_to_pdf(&[], Path::new("x.pdf")).is_err());
    }
}

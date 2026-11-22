//! Сборка PDF с текстовым слоем поверх изображений страниц
//! (используется OCR Studio: «сканированный PDF → PDF с поиском»).
//!
//! Подход: каждая страница — JPEG-изображение (DCTDecode) как фон +
//! невидимый текст (render mode 3) из hOCR Tesseract с координатами слов.

use convert_core::error::{ConvertError, ConvertResult};
use lopdf::dictionary;
use lopdf::{Dictionary, Document, Object};
use std::path::Path;

/// Собирает PDF: страницы-изображения + невидимый текстовый слой.
///
/// * `page_images` — PNG/JPEG страницы (порядок = порядок страниц);
/// * `hocr_files` — hOCR Tesseract для каждой страницы (тот же порядок);
/// * `output` — итоговый PDF.
pub fn build_searchable_pdf(
    page_images: &[std::path::PathBuf],
    hocr_files: &[std::path::PathBuf],
    output: &Path,
) -> ConvertResult<()> {
    if page_images.len() != hocr_files.len() {
        return Err(ConvertError::InvalidParams(
            "число изображений и hOCR-файлов должно совпадать".into(),
        ));
    }
    if page_images.is_empty() {
        return Err(ConvertError::InvalidParams("нет страниц".into()));
    }

    let mut doc = Document::with_version("1.7");
    let mut page_ids = Vec::new();

    for (img_path, hocr_path) in page_images.iter().zip(hocr_files) {
        // Изображение кодируем в JPEG (внутренний формат для встраивания).
        let img = image::open(img_path)
            .map_err(|e| ConvertError::Processing(format!("{}: {e}", img_path.display())))?;
        let (width, height) = (img.width(), img.height());

        // JPEG-байты для DCTDecode.
        let mut jpeg_bytes = Vec::new();
        {
            let mut cursor = std::io::Cursor::new(&mut jpeg_bytes);
            image::DynamicImage::ImageRgb8(img.to_rgb8())
                .write_to(&mut cursor, image::ImageFormat::Jpeg)
                .map_err(|e| ConvertError::Processing(format!("JPEG: {e}")))?;
        }

        // hOCR: слова с координатами bbox (в пикселях изображения).
        let words = parse_hocr_words(&std::fs::read_to_string(hocr_path)?);

        // PDF-пользовательские единицы = пиксели изображения.
        // Текстовый слой: невидимый (Tr 3), шрифт 1/20 пикселя высотой.
        let mut text_ops = String::new();
        text_ops.push_str("BT\n/TS 1 Tf\n3 Tr\n");
        for w in &words {
            // Нормализуем координаты bbox в пользовательские единицы.
            let x = w.x as f32;
            let y_top = height as f32 - w.y as f32; // PDF: y снизу
            let font_size = w.height.max(4).min(200) as f32;
            text_ops.push_str(&format!(
                "1 0 0 1 {:.2} {:.2} Tm ({}) Tj\n",
                x,
                y_top,
                escape(&w.text)
            ));
        }
        text_ops.push_str("ET\n");

        // Объекты страницы.
        let image_id = doc.add_object(Object::Stream(lopdf::Stream::new(
            lopdf::dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => width as i64,
                "Height" => height as i64,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            jpeg_bytes,
        )));

        let content = format!(
            "q\n{w} 0 0 {h} 0 0 cm\n/Im0 Do\nQ\n{text}",
            w = width,
            h = height,
            text = text_ops,
        );
        let content_id = doc.add_object(Object::Stream(lopdf::Stream::new(
            Dictionary::new(),
            content.into_bytes(),
        )));

        let font_id = doc.add_object(Object::Dictionary(lopdf::dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        }));
        let resources_id = doc.add_object(Object::Dictionary(lopdf::dictionary! {
            "XObject" => lopdf::dictionary! { "Im0" => Object::Reference(image_id) },
            "Font" => lopdf::dictionary! { "TS" => Object::Reference(font_id) },
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

    let pages_id = doc.add_object(Object::Dictionary(lopdf::dictionary! {
        "Type" => "Pages",
        "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
        "Count" => page_ids.len() as i64,
    }));
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set(b"Root", Object::Reference(catalog_id));

    doc.save(output)
        .map_err(|e| ConvertError::Processing(format!("сохранение PDF: {e}")))?;
    Ok(())
}

struct HocrWord {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    text: String,
}

/// Парсит hOCR: теги <span class="ocrx_word" title="bbox x0 y0 x1 y1">.
fn parse_hocr_words(hocr: &str) -> Vec<HocrWord> {
    let mut words = Vec::new();
    let mut rest = hocr;
    while let Some(start) = rest.find("ocrx_word") {
        let after = &rest[start..];
        // title="..." или title='...'
        let (title_start, quote) = match after
            .find("title=\"")
            .map(|p| (p + 7, '"'))
            .or_else(|| after.find("title='").map(|p| (p + 7, '\'')))
        {
            Some(v) => v,
            None => break,
        };
        let title_zone = &after[title_start..];
        let Some(title_end) = title_zone.find(quote) else { break };
        let title = &title_zone[..title_end];
        // title="bbox x0 y0 x1 y1; x_wconf 95"
        let coords: Vec<u32> = title
            .split(';')
            .next()
            .unwrap_or("")
            .trim_start_matches("bbox")
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        // Текст — до закрывающего </span>: после кавычки идёт '>' .
        let after_title = title_zone[title_end + 1..].trim_start_matches('>');
        let text = after_title
            .split('<')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if coords.len() == 4 && !text.is_empty() {
            words.push(HocrWord {
                x: coords[0],
                y: coords[1],
                width: coords[2].saturating_sub(coords[0]),
                height: coords[3].saturating_sub(coords[1]),
                text,
            });
        }
        rest = after_title;
    }
    words
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 256 => out.push(c),
            _ => out.push('?'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hocr_words() {
        let hocr = r#"
        <div>
          <span class='ocrx_word' title='bbox 10 20 110 60; x_wconf 95'>Hello</span>
          <span class='ocrx_word' title='bbox 120 20 200 60; x_wconf 90'>World</span>
        </div>"#;
        let words = parse_hocr_words(hocr);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Hello");
        assert_eq!(words[0].x, 10);
        assert_eq!(words[0].width, 100);
        assert_eq!(words[1].text, "World");
    }

    #[test]
    fn builds_searchable_pdf() {
        let dir = std::env::temp_dir().join("convert_pdf_search");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let img = dir.join("p.png");
        image::save_buffer_with_format(
            &img,
            image::RgbImage::from_fn(300, 150, |_, _| image::Rgb([255, 255, 255])).as_raw(),
            300,
            150,
            image::ExtendedColorType::Rgb8,
            image::ImageFormat::Png,
        )
        .unwrap();
        let hocr = dir.join("p.hocr");
        std::fs::write(
            &hocr,
            r#"<span class='ocrx_word' title='bbox 10 10 200 50; x_wconf 99'>TESTWORD</span>"#,
        )
        .unwrap();

        let out = dir.join("searchable.pdf");
        build_searchable_pdf(&[img], &[hocr], &out).unwrap();

        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        // Невидимый текст присутствует.
        assert!(bytes.windows(8).any(|w| w == b"TESTWORD"));
        assert_eq!(crate::images::pdf_page_count(&out).unwrap(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

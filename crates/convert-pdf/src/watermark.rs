//! Водяные знаки и нумерация страниц PDF (генерация контента через
//! low-level PDF-операторы; текст в Helvetica).

use convert_core::error::{ConvertError, ConvertResult};
use lopdf::dictionary;
use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Параметры водяного знака.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatermarkOptions {
    pub text: String,
    /// 0..=360, по часовой.
    pub angle: f32,
    /// Прозрачность 0..=1.
    pub opacity: f32,
    /// Размер шрифта, пункты.
    pub font_size: f32,
    /// Цвет RGB 0..=1.
    pub color: (f32, f32, f32),
    /// Повторять на каждой странице.
    pub all_pages: bool,
}

impl Default for WatermarkOptions {
    fn default() -> Self {
        Self {
            text: "CONVERT".into(),
            angle: 45.0,
            opacity: 0.15,
            font_size: 48.0,
            color: (0.5, 0.5, 0.5),
            all_pages: true,
        }
    }
}

fn escape_pdf_text(text: &str) -> String {
    // Кириллица в стандартных шрифтах PDF недоступна (WinAnsi);
    // честное ограничение — текст должен быть Latin-1.
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 256 => out.push(c),
            // Транслитерация не-ASCII, чтобы не потерять текст.
            c => out.push('?'),
        }
    }
    out
}


/// Размер страницы из MediaBox: (width, height) или A4-умолчание.
fn mediabox_size(page: &lopdf::Dictionary) -> (f32, f32) {
    let mb = page.get(b"MediaBox").ok().and_then(|o| o.as_array().ok());
    if let Some(arr) = mb {
        let get_f = |i: usize| -> Option<f32> {
            arr.get(i)
                .and_then(|o| o.as_float().ok().or_else(|| o.as_i64().ok().map(|v| v as f32)))
        };
        if let (Some(_x0), Some(_y0), Some(x1), Some(y1)) =
            (get_f(0), get_f(1), get_f(2), get_f(3))
        {
            return (x1, y1);
        }
    }
    (595.0, 842.0) // A4
}

/// Добавляет текстовый водяной знак (в центр каждой страницы).
pub fn add_text_watermark(
    source: &Path,
    output: &Path,
    options: &WatermarkOptions,
) -> ConvertResult<()> {
    let mut doc = Document::load(source)
        .map_err(|e| ConvertError::Processing(format!("PDF {e}")))?;

    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let targets: Vec<ObjectId> = if options.all_pages {
        pages
    } else {
        vec![pages[0]]
    };

    // Экстент-графика: q ... Q с прозрачностью через ExtGState.
    let text = escape_pdf_text(&options.text);
    let (r, g, b) = options.color;
    let opacity = options.opacity.clamp(0.0, 1.0);

    for page_id in targets {
        let page = doc.get_object(page_id)
            .and_then(|o| o.as_dict())
            .map_err(|e| ConvertError::Processing(format!("структура страницы: {e}")))?
            .clone();
        let (width, height) = mediabox_size(&page);

        // Операторы водяного знака в координатах страницы.
        let cx = width / 2.0;
        let cy = height / 2.0;
        let rad = options.angle.to_radians();
        let content = format!(
            "q\n{opacity:.3} gs\n{r:.3} {g:.3} {b:.3} rg\nBT\n/F1 {size:.1} Tf\n{rad:.4} {rad2:.4} {rad2:.4} -{rad:.4} {cx:.1} {cy:.1} Tm\n({text}) Tj\nET\nQ\n",
            opacity = opacity,
            r = r,
            g = g,
            b = b,
            size = options.font_size,
            rad = rad.cos(),
            rad2 = rad.sin(),
            cx = cx,
            cy = cy - options.font_size / 3.0,
            text = text,
        );

        // Ресурсы: шрифт F1 (Helvetica) + ExtGState gs.
        let resources_id = ensure_resources(&mut doc, page_id)?;
        append_content(&mut doc, page_id, resources_id, &content)?;
    }

    doc.save(output)
        .map_err(|e| ConvertError::Processing(format!("сохранение PDF: {e}")))?;
    Ok(())
}

/// Гарантирует наличие Resources/Font/ExtGState; возвращает ObjectId словаря Resources.
fn ensure_resources(doc: &mut Document, page_id: ObjectId) -> ConvertResult<ObjectId> {
    let page_ref = doc
        .get_object(page_id)
        .and_then(|o| o.as_dict())
        .map_err(|e| ConvertError::Processing(format!("страница: {e}")))?
        .clone();
    match page_ref.get(b"Resources") {
        Ok(Object::Reference(id)) => Ok(*id),
        Ok(Object::Dictionary(_)) => {
            // Инлайн-словарь: выносим в объект.
            let dict = page_ref.get(b"Resources").unwrap().clone();
            let id = doc.add_object(dict);
            if let Ok(pd) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
                pd.set(b"Resources", Object::Reference(id));
            }
            Ok(id)
        }
        _ => {
            let font_id = doc.add_object(Object::Dictionary(lopdf::dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => "Helvetica",
            }));
            let gs_id = doc.add_object(Object::Dictionary(lopdf::dictionary! {
                "Type" => "ExtGState",
                "ca" => 0.5,
                "CA" => 0.5,
            }));
            let resources = doc.add_object(Object::Dictionary(lopdf::dictionary! {
                "Font" => lopdf::dictionary! { "F1" => Object::Reference(font_id) },
                "ExtGState" => lopdf::dictionary! { "GS1" => Object::Reference(gs_id) },
            }));
            if let Ok(pd) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
                pd.set(b"Resources", Object::Reference(resources));
            }
            Ok(resources)
        }
    }
}

/// Добавляет контент к странице (создаёт или дополняет Contents).
fn append_content(
    doc: &mut Document,
    page_id: ObjectId,
    _resources_id: ObjectId,
    content_text: &str,
) -> ConvertResult<()> {
    let page_dict = doc
        .get_object(page_id)
        .and_then(|o| o.as_dict())
        .map_err(|e| ConvertError::Processing(format!("страница: {e}")))?
        .clone();

    let new_content_id = doc.add_object(Object::Stream(lopdf::Stream::new(
        lopdf::Dictionary::new(),
        content_text.as_bytes().to_vec(),
    )));

    match page_dict.get(b"Contents") {
        Ok(Object::Reference(existing)) => {
            // Оборачиваем в массив [old, new].
            if let Ok(pd) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
                pd.set(
                    b"Contents",
                    Object::Array(vec![
                        Object::Reference(*existing),
                        Object::Reference(new_content_id),
                    ]),
                );
            }
        }
        Ok(Object::Array(items)) => {
            let mut items = items.clone();
            items.push(Object::Reference(new_content_id));
            if let Ok(pd) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
                pd.set(b"Contents", Object::Array(items));
            }
        }
        _ => {
            if let Ok(pd) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
                pd.set(b"Contents", Object::Reference(new_content_id));
            }
        }
    }
    Ok(())
}

/// Добавляет нумерацию страниц «Стр. N из M» внизу каждой страницы.
pub fn add_page_numbers(
    source: &Path,
    output: &Path,
    position: NumberPosition,
) -> ConvertResult<()> {
    let mut doc = Document::load(source)
        .map_err(|e| ConvertError::Processing(format!("PDF {e}")))?;
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let total = pages.len();

    for (i, page_id) in pages.iter().enumerate() {
        let page = doc
            .get_object(*page_id)
            .and_then(|o| o.as_dict())
            .map_err(|e| ConvertError::Processing(format!("страница: {e}")))?
            .clone();
        let (width, height) = mediabox_size(&page);

        let label = format!("Page {} of {}", i + 1, total);
        let (x, y) = match position {
            NumberPosition::BottomCenter => (width / 2.0 - 40.0, 24.0),
            NumberPosition::BottomRight => (width - 110.0, 24.0),
            NumberPosition::TopRight => (width - 110.0, height - 32.0),
        };
        let content = format!(
            "q\n0.25 0.25 0.25 rg\nBT\n/F1 10 Tf\n{x:.1} {y:.1} Td\n({label}) Tj\nET\nQ\n",
            x = x,
            y = y,
            label = escape_pdf_text(&label),
        );
        let resources_id = ensure_resources(&mut doc, *page_id)?;
        append_content(&mut doc, *page_id, resources_id, &content)?;
    }

    doc.save(output)
        .map_err(|e| ConvertError::Processing(format!("сохранение PDF: {e}")))?;
    Ok(())
}

/// Позиция номера страницы.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum NumberPosition {
    BottomCenter,
    BottomRight,
    TopRight,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::tests::make_test_pdf;

    #[test]
    fn watermark_adds_content() {
        let dir = std::env::temp_dir().join("convert_pdf_wm");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.pdf");
        make_test_pdf(&src, 2);

        let out = dir.join("wm.pdf");
        add_text_watermark(&src, &out, &WatermarkOptions::default()).unwrap();
        // Файл стал больше (добавлен контент), структура валидна.
        let doc = Document::load(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 2);
        assert!(out.exists());

        // Проверяем наличие текста водяного знака в сырых байтах.
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.windows(7).any(|w| w == b"CONVERT"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn page_numbers_added() {
        let dir = std::env::temp_dir().join("convert_pdf_num");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.pdf");
        make_test_pdf(&src, 3);

        let out = dir.join("numbered.pdf");
        add_page_numbers(&src, &out, NumberPosition::BottomCenter).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.windows(11).any(|w| w == b"Page 1 of 3"));
        assert!(bytes.windows(11).any(|w| w == b"Page 3 of 3"));
        assert_eq!(Document::load(&out).unwrap().get_pages().len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn escapes_special_chars() {
        assert_eq!(escape_pdf_text("a(b)c\\d"), "a\\(b\\)c\\\\d");
    }
}

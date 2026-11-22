//! Извлечение текста из PDF: простой разбор content-потоков Tj/TJ.

use convert_core::error::{ConvertError, ConvertResult};
use lopdf::dictionary;
use lopdf::{Document, Object};
use std::path::Path;

/// Извлекает текст всего документа (страницы разделяются \n\n).
pub fn extract_text(path: &Path) -> ConvertResult<String> {
    let doc = Document::load(path)
        .map_err(|e| ConvertError::Processing(format!("PDF {e}")))?;
    let mut pages_text = Vec::new();
    for page_id in doc.get_pages().values() {
        pages_text.push(page_text_from_doc(&doc, *page_id));
    }
    Ok(pages_text.join("\n\n"))
}

/// Текст одной страницы (1-based).
pub fn page_text(path: &Path, page_number: usize) -> ConvertResult<String> {
    let doc = Document::load(path)
        .map_err(|e| ConvertError::Processing(format!("PDF {e}")))?;
    let pages: Vec<lopdf::ObjectId> = doc.get_pages().values().copied().collect();
    if page_number == 0 || page_number > pages.len() {
        return Err(ConvertError::InvalidParams(format!(
            "страница {page_number} вне границ 1..={}",
            pages.len()
        )));
    }
    Ok(page_text_from_doc(&doc, pages[page_number - 1]))
}

fn page_text_from_doc(doc: &Document, page_id: lopdf::ObjectId) -> String {
    let Ok(page) = doc.get_object(page_id).and_then(|o| o.as_dict()) else {
        return String::new();
    };
    // get_page_content сам собирает и декодирует все потоки Contents.
    let contents = doc.get_page_content(page_id).unwrap_or_default();

    // Разбор операторов: (текст) Tj  и  [(a) -20 (b)] TJ.
    let text = String::from_utf8_lossy(&contents);
    let mut result = String::new();
    let mut chars = text.char_indices().peekable();
    let mut pending_strings: Vec<String> = Vec::new();

    while let Some((i, c)) = chars.next() {
        match c {
            '(' => {
                // Строка с escape-последовательностями.
                let mut buf = String::new();
                let mut escaped = false;
                for (j, ch) in text[i + 1..].char_indices() {
                    if escaped {
                        match ch {
                            'n' => buf.push('\n'),
                            'r' => buf.push('\r'),
                            't' => buf.push('\t'),
                            other => buf.push(other),
                        }
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == ')' {
                        // Конец строки: пропускаем до этой позиции.
                        for _ in 0..(j + 1) {
                            chars.next();
                        }
                        break;
                    } else {
                        buf.push(ch);
                    }
                }
                pending_strings.push(buf);
            }
            'T' => {
                let rest = &text[i..];
                if rest.starts_with("Tj") || rest.starts_with("TJ") {
                    // Все накопленные строки выводим.
                    for s in pending_strings.drain(..) {
                        result.push_str(&s);
                    }
                    if rest.starts_with("Tj") {
                        result.push('\n');
                    }
                }
            }
            'E' => {
                // ET/T*: перенос строки.
                if text[i..].starts_with("ET") || text[i..].starts_with("TD")
                    || text[i..].starts_with("T*")
                {
                    result.push('\n');
                }
            }
            _ => {}
        }
    }
    // Очистка: пустые строки подряд.
    let mut cleaned = String::with_capacity(result.len());
    let mut empty_run = 0;
    for line in result.lines() {
        let t = line.trim();
        if t.is_empty() {
            empty_run += 1;
            if empty_run <= 1 {
                cleaned.push('\n');
            }
        } else {
            empty_run = 0;
            cleaned.push_str(t);
            cleaned.push('\n');
        }
    }
    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_from_generated_pdf() {
        // Собираем PDF с реальным текстовым контентом вручную.
        let dir = std::env::temp_dir().join("convert_pdf_text");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let content = b"BT /F1 12 Tf 72 720 Td (Hello PDF text) Tj ET";
        let mut doc = Document::with_version("1.7");
        let content_id = doc.add_object(Object::Stream(lopdf::Stream::new(
            lopdf::Dictionary::new(),
            content.to_vec(),
        )));
        let font_id = doc.add_object(Object::Dictionary(lopdf::dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        }));
        let resources_id = doc.add_object(Object::Dictionary(lopdf::dictionary! {
            "Font" => lopdf::dictionary! { "F1" => Object::Reference(font_id) },
        }));
        let page_id = doc.add_object(Object::Dictionary(lopdf::dictionary! {
            "Type" => "Page",
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Reference(resources_id),
            "MediaBox" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(612), Object::Integer(792)],
        }));
        let pages_id = doc.add_object(Object::Dictionary(lopdf::dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        }));
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set(b"Root", Object::Reference(catalog_id));
        let path = dir.join("text.pdf");
        doc.save(&path).unwrap();

        let text = extract_text(&path).unwrap();
        assert!(text.contains("Hello PDF text"), "извлечено: {text:?}");
        let page1 = page_text(&path, 1).unwrap();
        assert!(page1.contains("Hello PDF text"));
        assert!(page_text(&path, 2).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

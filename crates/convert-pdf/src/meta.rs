//! Метаданные PDF: чтение и редактирование (Title, Author, Subject,
//! Keywords, Creator, Producer).

use convert_core::error::{ConvertError, ConvertResult};
use lopdf::{Dictionary, Document, Object};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Метаданные PDF для UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PdfMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<String>,
    pub mod_date: Option<String>,
    pub page_count: usize,
    pub file_size: u64,
}

/// Читает метаданные PDF.
pub fn read_pdf_metadata(path: &Path) -> ConvertResult<PdfMetadata> {
    let doc = Document::load(path)
        .map_err(|e| ConvertError::Processing(format!("PDF {e}")))?;
    let info_ref = doc.trailer.get(b"Info").ok().and_then(|o| o.as_reference().ok());
    let info_dict: Dictionary = info_ref
        .and_then(|id| doc.get_object(id).ok())
        .and_then(|o| o.as_dict().ok())
        .cloned()
        .unwrap_or_else(Dictionary::new);

    let get_str = |dict: &Dictionary, key: &[u8]| -> Option<String> {
        // Строки PDF: Object::String(bytes) или Stream-словарь.
        dict.get(key).ok().and_then(|v| match v {
            Object::String(bytes, _) => Some(decode_pdf_string(bytes)),
            Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
            _ => None,
        }).filter(|s| !s.is_empty())
    };

    let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(PdfMetadata {
        title: get_str(&info_dict, b"Title"),
        author: get_str(&info_dict, b"Author"),
        subject: get_str(&info_dict, b"Subject"),
        keywords: get_str(&info_dict, b"Keywords"),
        creator: get_str(&info_dict, b"Creator"),
        producer: get_str(&info_dict, b"Producer"),
        creation_date: get_str(&info_dict, b"CreationDate"),
        mod_date: get_str(&info_dict, b"ModDate"),
        page_count: doc.get_pages().len(),
        file_size,
    })
}

/// Декодирует PDF-строку: BOM UTF-16LE или PDFDocEncoding ≈ Latin-1.
fn decode_pdf_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        // UTF-16BE с BOM.
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        // PDFDocEncoding: первые 256 кодов совпадают с Latin-1.
        String::from_utf8_lossy(bytes).to_string()
    }
}

/// Записывает метаданные в копию PDF.
pub fn set_pdf_metadata(
    source: &Path,
    output: &Path,
    meta: &PdfMetadata,
) -> ConvertResult<()> {
    let mut doc = Document::load(source)
        .map_err(|e| ConvertError::Processing(format!("PDF {e}")))?;

    let set_entry = |dict: &mut Dictionary, key: &[u8], value: &Option<String>| {
        match value {
            Some(v) if !v.is_empty() => {
                dict.set(key, Object::String(v.clone().into_bytes(), lopdf::StringFormat::Literal));
            }
            _ => {
                dict.remove(key);
            }
        }
    };

    // Находим или создаём Info.
    let info_ref = doc.trailer.get(b"Info").ok().and_then(|o| o.as_reference().ok());
    match info_ref {
        Some(id) => {
            if let Ok(dict) = doc.get_object_mut(id).and_then(|o| o.as_dict_mut()) {
                set_entry(dict, b"Title", &meta.title);
                set_entry(dict, b"Author", &meta.author);
                set_entry(dict, b"Subject", &meta.subject);
                set_entry(dict, b"Keywords", &meta.keywords);
                set_entry(dict, b"Creator", &meta.creator);
            }
        }
        None => {
            let mut dict = Dictionary::new();
            set_entry(&mut dict, b"Title", &meta.title);
            set_entry(&mut dict, b"Author", &meta.author);
            set_entry(&mut dict, b"Subject", &meta.subject);
            set_entry(&mut dict, b"Keywords", &meta.keywords);
            set_entry(&mut dict, b"Creator", &meta.creator);
            set_entry(&mut dict, b"Producer", &Some("CONVERT".to_string()));
            let id = doc.add_object(Object::Dictionary(dict));
            doc.trailer.set(b"Info", Object::Reference(id));
        }
    }

    doc.save(output)
        .map_err(|e| ConvertError::Processing(format!("сохранение PDF: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;
    use crate::pages::tests::make_test_pdf;

    #[test]
    fn metadata_roundtrip() {
        let dir = std::env::temp_dir().join("convert_pdf_meta");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.pdf");
        make_test_pdf(&src, 2);

        // Пустые метаданные у свежего файла.
        let meta = read_pdf_metadata(&src).unwrap();
        assert_eq!(meta.page_count, 2);

        // Записываем метаданные с кириллицей.
        let new_meta = PdfMetadata {
            title: Some("Тестовый документ".into()),
            author: Some("Иван Иванов".into()),
            subject: Some("Проверка".into()),
            keywords: Some("тест, pdf".into()),
            creator: Some("CONVERT".into()),
            producer: None,
            creation_date: None,
            mod_date: None,
            page_count: 0,
            file_size: 0,
        };
        let out = dir.join("out.pdf");
        set_pdf_metadata(&src, &out, &new_meta).unwrap();

        let read_back = read_pdf_metadata(&out).unwrap();
        assert_eq!(read_back.title.as_deref(), Some("Тестовый документ"));
        assert_eq!(read_back.author.as_deref(), Some("Иван Иванов"));
        assert_eq!(read_back.page_count, 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

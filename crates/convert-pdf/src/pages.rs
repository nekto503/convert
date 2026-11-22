//! Операции со страницами PDF: merge, split, extract, delete, reorder,
//! rotate — на чистом Rust через lopdf.
//!
//! Особенности lopdf 0.34:
//! - `get_pages()` возвращает BTreeMap<u32, ObjectId> (упорядочен по ключу);
//! - `delete_pages` доступен через трейт `Processor`.

use convert_core::error::{ConvertError, ConvertResult};
use lopdf::{dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Выбор страниц: «1-3,5,8-» (8- = с 8 до конца).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSelection(pub String);

impl PageSelection {
    /// Парсит строку в список индексов (0-based) для документа из
    /// `total` страниц.
    pub fn parse(&self, total: usize) -> ConvertResult<Vec<usize>> {
        let mut result = Vec::new();
        for part in self.0.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((a, b)) = part.split_once('-') {
                let start: usize = a.trim().parse().map_err(|_| {
                    ConvertError::InvalidParams(format!("некорректный диапазон: {part}"))
                })?;
                let end: usize = if b.trim().is_empty() {
                    total
                } else {
                    b.trim().parse().map_err(|_| {
                        ConvertError::InvalidParams(format!("некорректный диапазон: {part}"))
                    })?
                };
                if start == 0 || end < start || end > total {
                    return Err(ConvertError::InvalidParams(format!(
                        "диапазон {part} вне границ 1..={total}"
                    )));
                }
                result.extend(start - 1..end);
            } else {
                let n: usize = part.parse().map_err(|_| {
                    ConvertError::InvalidParams(format!("некорректный номер страницы: {part}"))
                })?;
                if n == 0 || n > total {
                    return Err(ConvertError::InvalidParams(format!(
                        "страница {n} вне границ 1..={total}"
                    )));
                }
                result.push(n - 1);
            }
        }
        if result.is_empty() {
            return Err(ConvertError::InvalidParams("не выбрано ни одной страницы".into()));
        }
        Ok(result)
    }
}

/// Открывает PDF с валидацией.
fn open_pdf(path: &Path) -> ConvertResult<Document> {
    Document::load(path)
        .map_err(|e| ConvertError::Processing(format!("не удалось открыть PDF {}: {e}", path.display())))
}

/// Упорядоченный список id страниц.
fn page_ids(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().values().copied().collect()
}

fn save_pdf(doc: &mut Document, path: &Path) -> ConvertResult<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    doc.save(path)
        .map_err(|e| ConvertError::Processing(format!("не удалось сохранить PDF: {e}")))
        .map(|_| ())
}

/// Сливает несколько PDF в один.
pub fn merge_pdfs(sources: &[std::path::PathBuf], output: &Path) -> ConvertResult<()> {
    if sources.is_empty() {
        return Err(ConvertError::InvalidParams("нет файлов для слияния".into()));
    }
    let mut merged = open_pdf(&sources[0])?;
    for src in &sources[1..] {
        let other = open_pdf(src)?;
        append_document(&mut merged, other)?;
    }
    save_pdf(&mut merged, output)
}

/// Переносит все страницы `other` в `target` (создаёт новые объекты
/// с новыми id, страницы добавляются в дерево страниц target).
fn append_document(target: &mut Document, other: Document) -> ConvertResult<()> {
    let other_pages = page_ids(&other);
    let mut new_page_ids = Vec::new();
    for page_id in other_pages {
        // Копируем объект страницы со всеми потомками в target.
        let new_id = copy_object_tree(target, &other, page_id)?;
        new_page_ids.push(new_id);
    }
    // Добавляем ссылки в дерево target.
    let catalog = target
        .catalog_mut()
        .map_err(|e| ConvertError::Processing(format!("каталог: {e}")))?;
    let pages_ref = catalog
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|e| ConvertError::Processing(format!("дерево страниц: {e}")))?;
    let pages_dict = target
        .get_object_mut(pages_ref)
        .and_then(Object::as_dict_mut)
        .map_err(|e| ConvertError::Processing(format!("дерево страниц: {e}")))?;
    let mut kids = match pages_dict.get(b"Kids") {
        Ok(Object::Array(a)) => a.clone(),
        _ => Vec::new(),
    };
    for id in new_page_ids {
        kids.push(Object::Reference(id));
    }
    let count = kids.len() as i64;
    pages_dict.set(b"Kids", Object::Array(kids));
    pages_dict.set(b"Count", Object::Integer(count));
    Ok(())
}

/// Рекурсивно копирует объект (и его непрямые ссылки) из src в target.
fn copy_object_tree(target: &mut Document, src: &Document, id: ObjectId) -> ConvertResult<ObjectId> {
    let obj = src
        .get_object(id)
        .map_err(|e| ConvertError::Processing(format!("объект {id:?}: {e}")))?;
    match obj {
        Object::Dictionary(dict) => {
            let mut new_dict = dictionary! {};
            for (k, v) in dict.iter() {
                let new_v = copy_value(target, src, v)?;
                new_dict.set(k.to_vec(), new_v);
            }
            Ok(target.add_object(Object::Dictionary(new_dict)))
        }
        Object::Stream(stream) => {
            let mut new_dict = dictionary! {};
            for (k, v) in stream.dict.iter() {
                let new_v = copy_value(target, src, v)?;
                new_dict.set(k.to_vec(), new_v);
            }
            let new_stream = lopdf::Stream::new(new_dict, stream.content.clone());
            Ok(target.add_object(Object::Stream(new_stream)))
        }
        other => Ok(target.add_object(other.clone())),
    }
}

fn copy_value(target: &mut Document, src: &Document, value: &Object) -> ConvertResult<Object> {
    match value {
        Object::Reference(id) => {
            let new_id = copy_object_tree(target, src, *id)?;
            Ok(Object::Reference(new_id))
        }
        Object::Array(items) => {
            let mut new_items = Vec::with_capacity(items.len());
            for item in items {
                new_items.push(copy_value(target, src, item)?);
            }
            Ok(Object::Array(new_items))
        }
        Object::Dictionary(dict) => {
            let mut new_dict = dictionary! {};
            for (k, v) in dict.iter() {
                let new_v = copy_value(target, src, v)?;
                new_dict.set(k.to_vec(), new_v);
            }
            Ok(Object::Dictionary(new_dict))
        }
        other => Ok(other.clone()),
    }
}

/// Разделяет PDF: каждая страница — отдельный файл в `output_dir`.
pub fn split_pdf(source: &Path, output_dir: &Path) -> ConvertResult<Vec<std::path::PathBuf>> {
    let doc = open_pdf(source)?;
    let total = page_ids(&doc).len();
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .map(convert_core::output::sanitize_filename)
        .unwrap_or_else(|| "page".into());
    std::fs::create_dir_all(output_dir)?;

    let mut outputs = Vec::new();
    for i in 0..total {
        let selection = PageSelection(format!("{})", i + 1).replace(')', ""));
        let out = output_dir.join(format!("{stem}_{:03}.pdf", i + 1));
        extract_pages(source, &PageSelection(format!("{}", i + 1)), &out)?;
        let _ = selection;
        outputs.push(out);
    }
    Ok(outputs)
}

/// Собирает новый PDF из выбранных страниц источника (0-based индексы)
/// через копирование деревьев объектов — надёжнее delete_pages, который
/// не всегда обновляет Kids в дереве страниц.
fn build_pdf_with_pages(source: &Path, indexes: &[usize], output: &Path) -> ConvertResult<()> {
    let src = open_pdf(source)?;
    let ids = page_ids(&src);
    let mut result = Document::with_version("1.7");
    let mut new_page_ids = Vec::new();
    for &i in indexes {
        if i >= ids.len() {
            return Err(ConvertError::InvalidParams(format!(
                "страница с индексом {i} вне границ (всего {})",
                ids.len()
            )));
        }
        let new_id = copy_object_tree(&mut result, &src, ids[i])?;
        new_page_ids.push(new_id);
    }
    let pages_id = result.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => Object::Array(new_page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>()),
        "Count" => new_page_ids.len() as i64,
    }));
    let catalog_id = result.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    result.trailer.set(b"Root", Object::Reference(catalog_id));
    save_pdf(&mut result, output)
}

/// Извлекает выбранные страницы в новый PDF.
pub fn extract_pages(source: &Path, selection: &PageSelection, output: &Path) -> ConvertResult<()> {
    let doc = open_pdf(source)?;
    let total = page_ids(&doc).len();
    let wanted = selection.parse(total)?;
    build_pdf_with_pages(source, &wanted, output)
}

/// Удаляет выбранные страницы (остальное — в новый файл).
pub fn delete_pages(source: &Path, selection: &PageSelection, output: &Path) -> ConvertResult<()> {
    let doc = open_pdf(source)?;
    let total = page_ids(&doc).len();
    let unwanted = selection.parse(total)?;
    let keep: Vec<usize> = (0..total).filter(|i| !unwanted.contains(i)).collect();
    if keep.is_empty() {
        return Err(ConvertError::InvalidParams(
            "нельзя удалить все страницы документа".into(),
        ));
    }
    build_pdf_with_pages(source, &keep, output)
}

/// Переставляет страницы в указанном порядке (строка «3,1,2»).
pub fn reorder_pages(source: &Path, order: &PageSelection, output: &Path) -> ConvertResult<()> {
    let doc = open_pdf(source)?;
    let total = page_ids(&doc).len();
    let order = order.parse(total)?;
    if order.len() != total {
        return Err(ConvertError::InvalidParams(format!(
            "перестановка должна включать все {total} страниц (получено {})",
            order.len()
        )));
    }
    build_pdf_with_pages(source, &order, output)
}

/// Поворачивает одну страницу на 90/180/270 градусов.
pub fn rotate_page(source: &Path, page_number: usize, degrees: u16, output: &Path) -> ConvertResult<()> {
    let mut doc = open_pdf(source)?;
    let pages = page_ids(&doc);
    if page_number == 0 || page_number > pages.len() {
        return Err(ConvertError::InvalidParams(format!(
            "страница {page_number} вне границ 1..={}",
            pages.len()
        )));
    }
    rotate_object(&mut doc, pages[page_number - 1], degrees);
    save_pdf(&mut doc, output)
}

/// Поворачивает все страницы.
pub fn rotate_all_pages(source: &Path, degrees: u16, output: &Path) -> ConvertResult<()> {
    let mut doc = open_pdf(source)?;
    let page_id_list = page_ids(&doc);
    for id in page_id_list {
        rotate_object(&mut doc, id, degrees);
    }
    save_pdf(&mut doc, output)
}

fn rotate_object(doc: &mut Document, page_id: ObjectId, degrees: u16) {
    if ![90, 180, 270].contains(&degrees) {
        return; // 0 или некорректное значение — без изменений
    }
    if let Ok(dict) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
        let current: i64 = dict.get(b"Rotate").and_then(|r| r.as_i64()).unwrap_or(0);
        let new_rotate = ((current + degrees as i64) % 360 + 360) % 360;
        if new_rotate != 0 {
            dict.set(b"Rotate", Object::Integer(new_rotate));
        } else {
            dict.remove(b"Rotate");
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Тестовый PDF: несколько страниц одним деревом.
    pub fn make_test_pdf(path: &Path, pages: usize) {
        let mut doc = Document::with_version("1.7");
        let mut kids: Vec<Object> = Vec::new();
        for _ in 0..pages {
            let content_id = doc.add_object(Object::Stream(lopdf::Stream::new(
                dictionary! {},
                b"BT (test) Tj ET".to_vec(),
            )));
            let resources_id = doc.add_object(Object::Dictionary(dictionary! {
                "Font" => dictionary! {
                    "F1" => Object::Name(b"Helvetica".to_vec()),
                },
            }));
            let page_id = doc.add_object(Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Contents" => Object::Reference(content_id),
                "Resources" => Object::Reference(resources_id),
                "MediaBox" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(612), Object::Integer(792)],
            }));
            kids.push(Object::Reference(page_id));
        }
        let pages_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(kids),
            "Count" => pages as i64,
        }));
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set(b"Root", Object::Reference(catalog_id));
        doc.save(path).unwrap();
    }

    #[test]
    fn page_selection_parsing() {
        let sel = PageSelection("1-3,5".into());
        assert_eq!(sel.parse(6).unwrap(), vec![0, 1, 2, 4]);
        let sel = PageSelection("2-".into());
        assert_eq!(sel.parse(4).unwrap(), vec![1, 2, 3]);
        assert!(PageSelection("0".into()).parse(3).is_err());
        assert!(PageSelection("5".into()).parse(3).is_err());
        assert!(PageSelection("3-1".into()).parse(3).is_err());
        assert!(PageSelection("".into()).parse(3).is_err());
    }

    #[test]
    fn merge_two_pdfs() {
        let dir = std::env::temp_dir().join("convert_pdf_merge");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.pdf");
        let b = dir.join("b.pdf");
        make_test_pdf(&a, 2);
        make_test_pdf(&b, 2);

        let merged = dir.join("merged.pdf");
        merge_pdfs(&[a, b], &merged).unwrap();
        let doc = open_pdf(&merged).unwrap();
        assert_eq!(doc.get_pages().len(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn split_extract_delete_rotate() {
        let dir = std::env::temp_dir().join("convert_pdf_ops");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.pdf");
        make_test_pdf(&src, 3);

        // Split: 3 файла.
        let parts = split_pdf(&src, &dir.join("parts")).unwrap();
        assert_eq!(parts.len(), 3);
        for p in &parts {
            assert_eq!(open_pdf(p).unwrap().get_pages().len(), 1);
        }

        // Extract страницы 1 и 3.
        let extracted = dir.join("extracted.pdf");
        extract_pages(&src, &PageSelection("1,3".into()), &extracted).unwrap();
        assert_eq!(open_pdf(&extracted).unwrap().get_pages().len(), 2);

        // Delete страницы 2 → остаётся 2.
        let deleted = dir.join("deleted.pdf");
        delete_pages(&src, &PageSelection("2".into()), &deleted).unwrap();
        assert_eq!(open_pdf(&deleted).unwrap().get_pages().len(), 2);

        // Reorder 3,2,1.
        let reordered = dir.join("reordered.pdf");
        reorder_pages(&src, &PageSelection("3,2,1".into()), &reordered).unwrap();
        assert_eq!(open_pdf(&reordered).unwrap().get_pages().len(), 3);

        // Rotate страницы 1 на 90°.
        let rotated = dir.join("rotated.pdf");
        rotate_page(&src, 1, 90, &rotated).unwrap();
        let doc = open_pdf(&rotated).unwrap();
        let page = doc.get_object(page_ids(&doc)[0]).unwrap();
        let rotate = page.as_dict().unwrap().get(b"Rotate").unwrap().as_i64().unwrap();
        assert_eq!(rotate, 90);

        // Rotate all 180° поверх 90 → 270.
        let rot2 = dir.join("rot2.pdf");
        rotate_all_pages(&rotated, 180, &rot2).unwrap();
        let doc2 = open_pdf(&rot2).unwrap();
        let page2 = doc2.get_object(page_ids(&doc2)[0]).unwrap();
        let rotate2 = page2.as_dict().unwrap().get(b"Rotate").unwrap().as_i64().unwrap();
        assert_eq!(rotate2, 270);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_rejected() {
        assert!(merge_pdfs(&[], Path::new("x.pdf")).is_err());
    }
}

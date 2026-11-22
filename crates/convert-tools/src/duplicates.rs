//! Duplicate Finder: поиск дубликатов по содержимому.
//!
//! Двухступенчатая стратегия для скорости на больших папках:
//! 1. группировка по размеру (файлы разного размера — не дубликаты);
//! 2. внутри групп — сравнение SHA-256 (частичный хэш первых 4 КБ для
//!    быстрой отбраковки, затем полный хэш).

use convert_core::error::ConvertResult;
use convert_core::hash::sha256_file;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Группа одинаковых файлов.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// SHA-256 содержимого.
    pub hash: String,
    /// Все копии (пути).
    pub files: Vec<PathBuf>,
    /// Размер одного экземпляра, байт.
    pub size: u64,
}

/// Сканирует папку рекурсивно и находит группы дубликатов.
pub fn find_duplicates(root: &Path) -> ConvertResult<Vec<DuplicateGroup>> {
    // 1. Все файлы с размерами.
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    collect_files(root, &mut by_size)?;

    let mut groups = Vec::new();
    for (_, files) in by_size {
        if files.len() < 2 {
            continue;
        }
        // 2. Частичный хэш (первые 4 КБ) — быстрая отбраковка.
        let mut by_partial: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for f in &files {
            let h = partial_hash(f);
            by_partial.entry(h).or_default().push(f.clone());
        }
        // 3. Полный хэш для прошедших отбор.
        for (_, candidates) in by_partial {
            if candidates.len() < 2 {
                continue;
            }
            let mut by_full: HashMap<String, Vec<PathBuf>> = HashMap::new();
            for f in &candidates {
                if let Ok(h) = sha256_file(f) {
                    by_full.entry(h).or_default().push(f.clone());
                }
            }
            for (hash, files) in by_full {
                if files.len() >= 2 {
                    let size = std::fs::metadata(&files[0]).map(|m| m.len()).unwrap_or(0);
                    groups.push(DuplicateGroup { hash, files, size });
                }
            }
        }
    }
    // Сортировка: самые «весомые» потери сверху.
    groups.sort_by(|a, b| b.size.cmp(&a.size));
    Ok(groups)
}

fn collect_files(root: &Path, by_size: &mut HashMap<u64, Vec<PathBuf>>) -> ConvertResult<()> {
    if root.is_file() {
        let size = std::fs::metadata(root)?.len();
        by_size.entry(size).or_default().push(root.to_path_buf());
        return Ok(());
    }
    let entries = std::fs::read_dir(root)
        .map_err(|e| convert_core::ConvertError::Processing(format!("{}: {e}", root.display())))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, by_size)?;
        } else if path.is_file() {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            by_size.entry(size).or_default().push(path);
        }
    }
    Ok(())
}

/// Хэш первых 4 КБ файла (с размером как солью).
fn partial_hash(path: &Path) -> String {
    use std::io::Read;
    let mut buf = [0u8; 4096];
    let mut s = String::new();
    if let Ok(mut f) = std::fs::File::open(path) {
        if let Ok(n) = f.read(&mut buf) {
            // Простой FNV-1a: не криптографический, но для отбора хватает.
            let mut hash: u64 = 0xcbf29ce484222325;
            for b in &buf[..n] {
                hash ^= *b as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
            s = format!("{hash:x}");
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_exact_duplicates() {
        let dir = std::env::temp_dir().join("convert_dup1");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("a.bin"), b"identical bytes here").unwrap();
        std::fs::write(dir.join("nested/b.bin"), b"identical bytes here").unwrap();
        std::fs::write(dir.join("c.bin"), b"different").unwrap();
        // Одинаковый размер, разное содержимое (проверит полный хэш).
        std::fs::write(dir.join("d.bin"), b"identical byte here!").unwrap();

        let groups = find_duplicates(&dir).unwrap();
        assert_eq!(groups.len(), 1, "ожидалась одна группа: {groups:?}");
        assert_eq!(groups[0].files.len(), 2);
        assert!(groups[0].files.iter().any(|f| f.ends_with("a.bin")));
        assert!(groups[0].files.iter().any(|f| f.ends_with("b.bin")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_duplicates_no_groups() {
        let dir = std::env::temp_dir().join("convert_dup2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("x"), b"1").unwrap();
        std::fs::write(dir.join("y"), b"2").unwrap();
        std::fs::write(dir.join("z"), b"3").unwrap();
        assert!(find_duplicates(&dir).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_dir_ok() {
        let dir = std::env::temp_dir().join("convert_dup3");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(find_duplicates(&dir).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

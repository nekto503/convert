//! Безопасное построение путей вывода.
//!
//! Правило продукта: результат сохраняется в ОТДЕЛЬНЫЙ файл; перезапись
//! исходника — только с явного подтверждения пользователя. Здесь живёт
//! логика `имя (2).ext`, защита от совпадения с исходником и очистка
//! недопустимых символов Windows.

use crate::error::{ConvertError, ConvertResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Как поступить с существующим файлом результата.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OverwriteMode {
    /// Не перезаписывать — добавить числовой суффикс `имя (2).ext`. По умолчанию.
    #[default]
    Rename,
    /// Перезаписать файл результата (но никогда — исходный).
    Overwrite,
}

/// Режим расположения результата.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputMode {
    /// Рядом с исходным файлом (по умолчанию).
    NextToSource,
    /// В указанную пользователем папку.
    Directory(PathBuf),
}

/// Убирает из имени файла символы, недопустимые в Windows:
/// `< > : " / \ | ? *` и управляющие символы.
pub fn sanitize_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => out.push('_'),
            c if (c as u32) < 0x20 => out.push('_'),
            c => out.push(c),
        }
    }
    // Windows не любит точки и пробелы в конце имени.
    let trimmed = out.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        "file".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Строит путь файла результата.
///
/// * `source` — исходный файл;
/// * `output_dir` — `None` = рядом с исходником, `Some(dir)` = в папку;
/// * `new_extension` — новое расширение без точки (или `None`, чтобы
///   оставить прежнее);
/// * `suffix` — необязательный суффикс имени, например `_compressed`;
/// * `overwrite` — режим перезаписи существующего результата.
///
/// Функция гарантирует: результат никогда не равен пути исходника
/// (иначе — ошибка, требующая подтверждения пользователя на уровне UI).
pub fn build_output_path(
    source: &Path,
    output_dir: Option<&Path>,
    new_extension: Option<&str>,
    suffix: Option<&str>,
    overwrite: OverwriteMode,
) -> ConvertResult<PathBuf> {
    if !source.is_file() {
        return Err(ConvertError::NotAFile(source.display().to_string()));
    }

    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .map(sanitize_filename)
        .unwrap_or_else(|| "file".to_string());
    let ext = new_extension
        .map(|e| sanitize_extension(e))
        .or_else(|| source.extension().and_then(|e| e.to_str()).map(|e| e.to_string()))
        .unwrap_or_else(|| "bin".to_string());

    let dir = match output_dir {
        Some(d) => d.to_path_buf(),
        None => source
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    // Создаём папку назначения, если её нет.
    std::fs::create_dir_all(&dir)?;

    let file_name = match suffix {
        Some(s) => format!("{stem}{s}.{ext}"),
        None => format!("{stem}.{ext}"),
    };
    let mut result = dir.join(&file_name);

    // Ключевая защита: результат не должен совпасть с исходником.
    let same_as_source = paths_equal(&result, source);
    if same_as_source && overwrite == OverwriteMode::Overwrite {
        return Err(ConvertError::InvalidParams(
            "результат совпал с исходным файлом: выберите другую папку или формат".into(),
        ));
    }
    if same_as_source {
        // Рядом с исходником в тот же формат — добавляем суффикс.
        result = dir.join(format!("{stem}_converted.{ext}"));
    }

    if result.exists() {
        match overwrite {
            OverwriteMode::Overwrite => { /* путь используется как есть */ }
            OverwriteMode::Rename => {
                result = unique_path(&result);
            }
        }
    }
    Ok(result)
}

/// Подбирает `имя (2).ext`, `имя (3).ext`, ... для существующего пути.
fn unique_path(path: &Path) -> PathBuf {
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    for n in 2..u32::MAX {
        let candidate = if ext.is_empty() {
            dir.join(format!("{stem} ({n})"))
        } else {
            dir.join(format!("{stem} ({n}).{ext}"))
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    // Практически недостижимо, но оставим детерминированный ответ.
    dir.join(format!("{stem}_{}.{}", stem, ext))
}

fn sanitize_extension(ext: &str) -> String {
    let cleaned: String = ext
        .trim()
        .trim_start_matches('.')
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if cleaned.is_empty() {
        "bin".to_string()
    } else {
        cleaned.to_lowercase()
    }
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    // Канонизируем, чтобы сравнивать реальные пути, а не строки.
    let ca = std::fs::canonicalize(a);
    let cb = std::fs::canonicalize(b);
    match (ca, cb) {
        (Ok(a), Ok(b)) => a == b,
        // Один из путей ещё не существует — сравниваем лексически.
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_removes_windows_forbidden() {
        assert_eq!(sanitize_filename("a<b>c:d\"e/f\\g|h?i*.txt"), "a_b_c_d_e_f_g_h_i_.txt");
        assert_eq!(sanitize_filename("name... "), "name");
        assert_eq!(sanitize_filename("   "), "file");
    }

    #[test]
    fn output_next_to_source_changes_extension() {
        let dir = std::env::temp_dir().join("convert_core_tests_out1");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("photo.png");
        std::fs::write(&src, b"png").unwrap();

        let out = build_output_path(&src, None, Some("jpg"), None, OverwriteMode::Rename).unwrap();
        assert_eq!(out, dir.join("photo.jpg"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn never_overwrites_source() {
        let dir = std::env::temp_dir().join("convert_core_tests_out2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("photo.png");
        std::fs::write(&src, b"png").unwrap();

        // Тот же формат, та же папка → суффикс _converted.
        let out = build_output_path(&src, None, Some("png"), None, OverwriteMode::Rename).unwrap();
        assert!(out != src);
        assert!(out.file_name().unwrap().to_str().unwrap().contains("converted"));

        // Даже с Overwrite исходник трогать нельзя.
        let err = build_output_path(&src, None, Some("png"), None, OverwriteMode::Overwrite);
        assert!(err.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_target_gets_number() {
        let dir = std::env::temp_dir().join("convert_core_tests_out3");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("a.png");
        std::fs::write(&src, b"png").unwrap();
        std::fs::write(dir.join("a.jpg"), b"x").unwrap();

        let out = build_output_path(&src, None, Some("jpg"), None, OverwriteMode::Rename).unwrap();
        assert_eq!(out, dir.join("a (2).jpg"));

        // С перезаписью — тот же путь.
        let out2 = build_output_path(&src, None, Some("jpg"), None, OverwriteMode::Overwrite).unwrap();
        assert_eq!(out2, dir.join("a.jpg"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn custom_directory_is_created() {
        let base = std::env::temp_dir().join("convert_core_tests_out4");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let src = base.join("x.png");
        std::fs::write(&src, b"png").unwrap();

        let target_dir = base.join("nested/output");
        let out = build_output_path(&src, Some(&target_dir), Some("webp"), None, OverwriteMode::Rename).unwrap();
        assert_eq!(out, target_dir.join("x.webp"));
        assert!(target_dir.is_dir());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn suffix_appended() {
        let dir = std::env::temp_dir().join("convert_core_tests_out5");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("doc.pdf");
        std::fs::write(&src, b"pdf").unwrap();

        let out = build_output_path(&src, None, None, Some("_compressed"), OverwriteMode::Rename).unwrap();
        assert_eq!(out, dir.join("doc_compressed.pdf"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

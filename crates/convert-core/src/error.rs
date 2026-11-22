//! Единая система ошибок CONVERT.
//!
//! Каждый движок возвращает `ConvertError`, а оболочка Tauri превращает её
//! в строку для frontend. Никаких паник в логике обработки файлов: любая
//! ошибка — это значение, которое можно показать пользователю.

use serde::{Serialize, Serializer};
use thiserror::Error;

/// Результат любой операции CONVERT: успех несёт `T`, ошибка — `ConvertError`.
pub type ConvertResult<T> = Result<T, ConvertError>;

#[derive(Debug, Error)]
pub enum ConvertError {
    /// Файл не найден или недоступен.
    #[error("файл не найден или недоступен: {0}")]
    FileNotFound(String),

    /// Путь указывает не на файл.
    #[error("это не файл: {0}")]
    NotAFile(String),

    /// Файл пустой (0 байт) — обрабатывать нечего.
    #[error("файл пустой: {0}")]
    EmptyFile(String),

    /// Не удалось прочитать данные (ошибка ввода-вывода).
    #[error("ошибка ввода-вывода: {0}")]
    Io(#[from] std::io::Error),

    /// Формат файла не распознан.
    #[error("не удалось определить формат файла: {0}")]
    UnknownFormat(String),

    /// Формат распознан, но такой входной формат движок не принимает.
    #[error("неподдерживаемый входной формат: {0}")]
    UnsupportedInput(String),

    /// Целевой формат не поддерживается для этого входного.
    #[error("недоступен целевой формат: {0}")]
    UnsupportedTarget(String),

    /// Преобразование возможно, но приведёт к потере данных (прозрачность,
    /// слои, качество). Требует явного согласия пользователя.
    #[error("преобразование приведёт к потере данных: {0}")]
    LossyConversion(String),

    /// Нарушены ограничения архива: zip-бомба, слишком много данных.
    #[error("архив небезопасен: {0}")]
    UnsafeArchive(String),

    /// Некорректные параметры операции (нулевая ширина, битрейт и т.п.).
    #[error("некорректные параметры: {0}")]
    InvalidParams(String),

    /// Внешний движок (ffmpeg/soffice/tesseract) не найден в системе.
    #[error("не найден внешний компонент: {0}")]
    EngineMissing(String),

    /// Внешний движок завершился с ошибкой.
    #[error("движок {engine} завершился с ошибкой (код {code}): {stderr}")]
    EngineFailed {
        engine: String,
        code: i32,
        stderr: String,
    },

    /// Ошибка кодирования/декодирования изображения или документа.
    #[error("ошибка обработки данных: {0}")]
    Processing(String),

    /// Операция отменена пользователем.
    #[error("операция отменена")]
    Cancelled,

    /// Любая другая ошибка, не попадающая в категории выше.
    #[error("{0}")]
    Other(String),
}

impl ConvertError {
    /// Превращает ошибку в строку для показа пользователю.
    pub fn user_message(&self) -> String {
        self.to_string()
    }
}

// Frontend получает ошибки как обычные строки через Tauri-команды,
// поэтому сериализуем через Display.
impl Serialize for ConvertError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_are_russian() {
        let e = ConvertError::FileNotFound("test.png".into());
        assert!(e.user_message().contains("не найден"));
    }

    #[test]
    fn engine_error_includes_stderr() {
        let e = ConvertError::EngineFailed {
            engine: "ffmpeg".into(),
            code: 1,
            stderr: "Invalid data".into(),
        };
        let msg = e.user_message();
        assert!(msg.contains("ffmpeg") && msg.contains("Invalid data"));
    }
}

//! convert-core — фундамент всего приложения CONVERT.
//!
//! Здесь живёт то, что нужно каждому движку обработки:
//! - детекция формата файла по содержимому (magic bytes), а не по расширению;
//! - единая система ошибок `ConvertError`;
//! - матрица совместимости форматов и рекомендации по конвертации;
//! - безопасное построение пути вывода (защита от перезаписи исходника);
//! - типы настроек, которые сериализуются в SQLite и передаются из frontend.
//!
//! Правило проекта: тяжёлая обработка файлов живёт в Rust-движках,
//! frontend только отображает данные и вызывает Tauri-команды.

pub mod compat;
pub mod error;
pub mod format;
pub mod hash;
pub mod output;
pub mod settings;

pub use compat::{compatibility, conversion_targets, recommended_target, ConversionTarget, FormatQuality};
pub use error::{ConvertError, ConvertResult};
pub use format::{
    detect_format, detect_format_path, detect_format_reader, FileCategory, FileFormat,
};
pub use hash::sha256_file;
pub use output::{build_output_path, sanitize_filename, OverwriteMode, OutputMode};
pub use settings::{
    AppSettings, AppearanceSettings, BatchSettings, HistorySettings, PrivacySettings,
    ProcessingSettings,
};

/// Пере-экспорт внутренностей error для движков (единая система ошибок).
pub use error as public_error;

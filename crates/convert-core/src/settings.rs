//! Настройки приложения. Хранятся в SQLite (см. convert-db) и передаются
//! из frontend в виде JSON. Структуры сериализуемы в обе стороны.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Корневые настройки приложения CONVERT.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub appearance: AppearanceSettings,
    pub processing: ProcessingSettings,
    pub batch: BatchSettings,
    pub history: HistorySettings,
    pub privacy: PrivacySettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            appearance: AppearanceSettings::default(),
            processing: ProcessingSettings::default(),
            batch: BatchSettings::default(),
            history: HistorySettings::default(),
            privacy: PrivacySettings::default(),
        }
    }
}

/// Внешний вид: тема и акцентный цвет.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceSettings {
    /// "light" | "dark" | "system"
    pub theme: String,
    /// Акцентный цвет в формате "#RRGGBB".
    pub accent: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            accent: "#4F7CFF".to_string(),
        }
    }
}

/// Общие параметры обработки файлов.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingSettings {
    /// Куда сохранять результат по умолчанию: None = рядом с исходником.
    pub output_directory: Option<PathBuf>,
    /// Перезаписывать существующие результаты (пользователь подтверждает в UI).
    pub overwrite_results: bool,
    /// Останавливать пакетную обработку на первой ошибке.
    pub stop_on_error: bool,
    /// Папка для временных файлов (None = системная TEMP).
    pub temp_directory: Option<PathBuf>,
    /// Автоматически удалять временные файлы после операции.
    pub cleanup_temp: bool,
}

impl Default for ProcessingSettings {
    fn default() -> Self {
        Self {
            output_directory: None,
            overwrite_results: false,
            stop_on_error: false,
            temp_directory: None,
            cleanup_temp: true,
        }
    }
}

/// Пакетная обработка: защита от перегрузки системы.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSettings {
    /// Число параллельных задач (воркеров). 0 = авто (по числу ядер).
    pub max_parallel_tasks: usize,
    /// Максимальный размер распаковываемых архивов, байт (защита от zip-бомб).
    pub archive_extract_limit_bytes: u64,
    /// Максимальное число файлов в распаковываемом архиве.
    pub archive_extract_limit_files: u64,
}

impl Default for BatchSettings {
    fn default() -> Self {
        Self {
            max_parallel_tasks: 0,
            archive_extract_limit_bytes: 8 * 1024 * 1024 * 1024, // 8 ГБ
            archive_extract_limit_files: 100_000,
        }
    }
}

/// История операций.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySettings {
    /// Сохранять историю вообще.
    pub enabled: bool,
    /// Хранить не дольше N дней (0 = бессрочно).
    pub retention_days: u32,
    /// Максимум записей (0 = без ограничения).
    pub max_entries: u32,
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_days: 90,
            max_entries: 10_000,
        }
    }
}

/// Конфиденциальность: всё локально, никакой телеметрии по умолчанию.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    /// Телеметрия выключена по умолчанию и никуда не отправляется,
    /// пока пользователь не включит её явно.
    pub telemetry_enabled: bool,
    /// Разрешить проверку обновлений (обращение к сети).
    pub check_updates: bool,
    /// Отправлять файлы на внешние сервисы (всегда только с явного согласия).
    pub allow_external_processing: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            telemetry_enabled: false,
            check_updates: false,
            allow_external_processing: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_privacy_first() {
        let s = AppSettings::default();
        assert!(!s.privacy.telemetry_enabled);
        assert!(!s.privacy.allow_external_processing);
        assert!(s.history.enabled);
        assert_eq!(s.appearance.theme, "system");
    }

    #[test]
    fn settings_roundtrip_json() {
        let s = AppSettings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.batch.max_parallel_tasks, s.batch.max_parallel_tasks);
        assert_eq!(back.appearance.accent, s.appearance.accent);
    }
}

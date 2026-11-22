//! Контекст приложения: база данных, очередь, настройки.
//! Единственный владелец изменяемого состояния; Tauri оборачивает его
//! в Manage.

use convert_core::AppSettings;
use convert_db::Database;
use convert_pipeline::{BatchQueue, TaskRunner};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Глобальное состояние CONVERT.
pub struct AppContext {
    pub db: Mutex<Database>,
    pub queue: Arc<BatchQueue>,
    /// Каталог данных приложения (база, логи).
    pub data_dir: PathBuf,
}

impl AppContext {
    /// Создаёт контекст в указанном каталоге данных.
    ///
    /// `runner` — исполнитель задач (см. runner::build_runner);
    /// в тестах подставляется тестовый.
    pub fn new(data_dir: PathBuf, runner: TaskRunner, max_workers: usize) -> convert_core::ConvertResult<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let db = Database::open(&data_dir.join("convert.db"))?;
        let queue = BatchQueue::new(runner, max_workers);
        queue.start();
        Ok(Self {
            db: Mutex::new(db),
            queue,
            data_dir,
        })
    }

    /// Текущие настройки (из БД или дефолтные).
    pub fn settings(&self) -> AppSettings {
        let db = self.db.lock().unwrap();
        db.get_setting_json("app_settings")
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Сохранить настройки.
    pub fn set_settings(&self, settings: &AppSettings) -> convert_core::ConvertResult<()> {
        let db = self.db.lock().unwrap();
        db.set_setting_json("app_settings", settings)
    }
}

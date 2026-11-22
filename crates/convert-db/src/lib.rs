//! convert-db — хранение данных CONVERT в SQLite:
//! история операций (File History), настройки, очередь задач (batch),
//! сохранённые пресеты и сценарии Workflow.
//!
//! Схема миграциями вперёд: `migrate()` создаёт недостающие таблицы,
//! не трогая существующие данные.

use convert_core::error::{ConvertError, ConvertResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Обёртка над соединением с SQLite.
pub struct Database {
    conn: Connection,
}

/// Запись истории операций (ТЗ раздел 7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: Option<i64>,
    /// Название операции: «PNG → WEBP», «Сжатие видео»...
    pub operation: String,
    /// Инструмент: image_studio / video_studio / ...
    pub tool: String,
    /// Исходный файл.
    pub source: String,
    /// Файл результата (NULL при ошибке).
    pub output: Option<String>,
    /// Статус: success / error / cancelled.
    pub status: String,
    /// Сообщение об ошибке.
    pub error: Option<String>,
    /// Настройки операции (JSON).
    pub settings_json: Option<String>,
    /// Время выполнения, мс.
    pub duration_ms: u64,
    /// Метка времени Unix.
    pub timestamp: i64,
}

/// Запись очереди пакетной обработки.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    pub id: Option<i64>,
    /// Тип задачи: convert_image / convert_video / ...
    pub task_type: String,
    /// Параметры задачи (JSON).
    pub payload_json: String,
    /// Статус: pending / running / done / error / cancelled.
    pub status: String,
    /// Позиция в очереди.
    pub position: i64,
    pub error: Option<String>,
    pub timestamp: i64,
}

impl Database {
    /// Открывает (или создаёт) базу в указанном файле.
    pub fn open(path: &Path) -> ConvertResult<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)
            .map_err(|e| ConvertError::Processing(format!("SQLite: {e}")))?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// База в памяти (тесты, предпросмотр).
    pub fn open_in_memory() -> ConvertResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| ConvertError::Processing(format!("SQLite: {e}")))?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> ConvertResult<()> {
        self.conn
            .execute_batch(
                r#"
                PRAGMA journal_mode = WAL;

                CREATE TABLE IF NOT EXISTS history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    operation TEXT NOT NULL,
                    tool TEXT NOT NULL,
                    source TEXT NOT NULL,
                    output TEXT,
                    status TEXT NOT NULL,
                    error TEXT,
                    settings_json TEXT,
                    duration_ms INTEGER NOT NULL DEFAULT 0,
                    timestamp INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_history_time ON history(timestamp DESC);
                CREATE INDEX IF NOT EXISTS idx_history_source ON history(source);

                CREATE TABLE IF NOT EXISTS queue (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_type TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending',
                    position INTEGER NOT NULL DEFAULT 0,
                    error TEXT,
                    timestamp INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS settings (
                    key TEXT PRIMARY KEY,
                    value_json TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS presets (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    tool TEXT NOT NULL,
                    settings_json TEXT NOT NULL,
                    timestamp INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS workflows (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    steps_json TEXT NOT NULL,
                    timestamp INTEGER NOT NULL
                );
                "#,
            )
            .map_err(|e| ConvertError::Processing(format!("миграция: {e}")))?;
        Ok(())
    }

    // ---- История -------------------------------------------------------------

    /// Добавляет запись истории.
    pub fn add_history(&self, entry: &HistoryEntry) -> ConvertResult<i64> {
        self.conn
            .execute(
                "INSERT INTO history (operation, tool, source, output, status, error, settings_json, duration_ms, timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    entry.operation,
                    entry.tool,
                    entry.source,
                    entry.output,
                    entry.status,
                    entry.error,
                    entry.settings_json,
                    entry.duration_ms as i64,
                    entry.timestamp,
                ],
            )
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// История с поиском и фильтрами.
    ///
    /// * `query` — подстрока в operation/source/output;
    /// * `status` — фильтр по статусу;
    /// * `limit` — максимум записей.
    pub fn history(
        &self,
        query: Option<&str>,
        status: Option<&str>,
        limit: u32,
    ) -> ConvertResult<Vec<HistoryEntry>> {
        let mut sql = String::from(
            "SELECT id, operation, tool, source, output, status, error, settings_json, duration_ms, timestamp
             FROM history WHERE 1=1",
        );
        if query.is_some() {
            sql.push_str(" AND (operation LIKE ?1 OR source LIKE ?1 OR output LIKE ?1)");
        }
        if status.is_some() {
            sql.push_str(" AND status = ?2");
        }
        sql.push_str(" ORDER BY timestamp DESC LIMIT ?3");

        let like = query.map(|q| format!("%{q}%"));
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        let rows = stmt
            .query_map(
                rusqlite::params_from_iter([
                    like.unwrap_or_default(),
                    status.unwrap_or("").to_string(),
                    limit.to_string(),
                ]),
                |row| {
                    Ok(HistoryEntry {
                        id: row.get(0)?,
                        operation: row.get(1)?,
                        tool: row.get(2)?,
                        source: row.get(3)?,
                        output: row.get(4)?,
                        status: row.get(5)?,
                        error: row.get(6)?,
                        settings_json: row.get(7)?,
                        duration_ms: row.get::<_, i64>(8)? as u64,
                        timestamp: row.get(9)?,
                    })
                },
            )
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| ConvertError::Processing(e.to_string()))?);
        }
        Ok(result)
    }

    /// Удаляет запись истории.
    pub fn delete_history(&self, id: i64) -> ConvertResult<()> {
        self.conn
            .execute("DELETE FROM history WHERE id = ?1", params![id])
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        Ok(())
    }

    /// Полная очистка истории.
    pub fn clear_history(&self) -> ConvertResult<()> {
        self.conn
            .execute("DELETE FROM history", [])
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        Ok(())
    }

    /// Чистка по правилам хранения (дни/максимум записей).
    pub fn apply_history_retention(&self, retention_days: u32, max_entries: u32) -> ConvertResult<()> {
        if retention_days > 0 {
            let cutoff = now_ts() - retention_days as i64 * 86_400;
            self.conn
                .execute("DELETE FROM history WHERE timestamp < ?1", params![cutoff])
                .map_err(|e| ConvertError::Processing(e.to_string()))?;
        }
        if max_entries > 0 {
            self.conn
                .execute(
                    "DELETE FROM history WHERE id NOT IN (
                        SELECT id FROM history ORDER BY timestamp DESC LIMIT ?1
                    )",
                    params![max_entries],
                )
                .map_err(|e| ConvertError::Processing(e.to_string()))?;
        }
        Ok(())
    }

    // ---- Очередь -------------------------------------------------------------

    /// Добавляет задачу в конец очереди.
    pub fn enqueue(&self, task_type: &str, payload_json: &str) -> ConvertResult<i64> {
        let pos: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(position), 0) FROM queue", [], |r| r.get(0))
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        self.conn
            .execute(
                "INSERT INTO queue (task_type, payload_json, status, position, timestamp)
                 VALUES (?1, ?2, 'pending', ?3, ?4)",
                params![task_type, payload_json, pos + 1, now_ts()],
            )
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Достаёт следующую pending-задачу (наименьший position).
    pub fn dequeue_next(&self) -> ConvertResult<Option<QueueEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, task_type, payload_json, status, position, error, timestamp
                 FROM queue WHERE status = 'pending' ORDER BY position ASC LIMIT 1",
            )
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        let mut rows = stmt
            .query_map([], row_to_queue)
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        if let Some(row) = rows.next() {
            let entry = row.map_err(|e| ConvertError::Processing(e.to_string()))?;
            self.set_queue_status(entry.id.unwrap(), "running", None)?;
            Ok(Some(entry))
        } else {
            Ok(None)
        }
    }

    /// Все задачи очереди (для UI).
    pub fn queue_all(&self) -> ConvertResult<Vec<QueueEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, task_type, payload_json, status, position, error, timestamp FROM queue ORDER BY position ASC")
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        let rows = stmt
            .query_map([], row_to_queue)
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| ConvertError::Processing(e.to_string()))?);
        }
        Ok(result)
    }

    /// Обновляет статус задачи.
    pub fn set_queue_status(&self, id: i64, status: &str, error: Option<&str>) -> ConvertResult<()> {
        self.conn
            .execute(
                "UPDATE queue SET status = ?2, error = ?3 WHERE id = ?1",
                params![id, status, error],
            )
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        Ok(())
    }

    /// Сбрасывает «зависшие» running-задачи в pending (восстановление
    /// очереди после перезапуска приложения, ТЗ раздел 6).
    pub fn recover_interrupted(&self) -> ConvertResult<usize> {
        let n = self
            .conn
            .execute("UPDATE queue SET status = 'pending' WHERE status = 'running'", [])
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        Ok(n)
    }

    /// Удаляет завершённые задачи.
    pub fn clean_finished_queue(&self) -> ConvertResult<()> {
        self.conn
            .execute("DELETE FROM queue WHERE status IN ('done', 'cancelled')", [])
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        Ok(())
    }

    // ---- Настройки ------------------------------------------------------------

    /// Сохраняет значение настройки (JSON).
    pub fn set_setting(&self, key: &str, value_json: &str) -> ConvertResult<()> {
        self.conn
            .execute(
                "INSERT INTO settings (key, value_json) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value_json = ?2",
                params![key, value_json],
            )
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        Ok(())
    }

    /// Читает настройку.
    pub fn get_setting(&self, key: &str) -> ConvertResult<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value_json FROM settings WHERE key = ?1")
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        let mut rows = stmt
            .query_map(params![key], |r| r.get::<_, String>(0))
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        if let Some(row) = rows.next() {
            Ok(Some(row.map_err(|e| ConvertError::Processing(e.to_string()))?))
        } else {
            Ok(None)
        }
    }

    /// Сохраняет объект настройки сериализацией в JSON.
    pub fn set_setting_json<T: Serialize>(&self, key: &str, value: &T) -> ConvertResult<()> {
        let json = serde_json::to_string(value)
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        self.set_setting(key, &json)
    }

    /// Читает объект настройки.
    pub fn get_setting_json<T: for<'de> Deserialize<'de>>(&self, key: &str) -> ConvertResult<Option<T>> {
        match self.get_setting(key)? {
            Some(json) => serde_json::from_str(&json)
                .map(Some)
                .map_err(|e| ConvertError::Processing(e.to_string())),
            None => Ok(None),
        }
    }

    // ---- Пресеты и сценарии -----------------------------------------------------

    /// Сохраняет пресет настроек инструмента.
    pub fn save_preset(&self, name: &str, tool: &str, settings_json: &str) -> ConvertResult<i64> {
        self.conn
            .execute(
                "INSERT INTO presets (name, tool, settings_json, timestamp)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(name) DO UPDATE SET settings_json = ?3, timestamp = ?4",
                params![name, tool, settings_json, now_ts()],
            )
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Список пресетов инструмента.
    pub fn presets(&self, tool: &str) -> ConvertResult<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, settings_json FROM presets WHERE tool = ?1 ORDER BY name")
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        let rows = stmt
            .query_map(params![tool], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| ConvertError::Processing(e.to_string()))?);
        }
        Ok(result)
    }

    /// Удаляет пресет.
    pub fn delete_preset(&self, name: &str) -> ConvertResult<()> {
        self.conn
            .execute("DELETE FROM presets WHERE name = ?1", params![name])
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        Ok(())
    }

    /// Сохраняет сценарий Workflow (JSON-описание шагов).
    pub fn save_workflow(&self, name: &str, steps_json: &str) -> ConvertResult<i64> {
        self.conn
            .execute(
                "INSERT INTO workflows (name, steps_json, timestamp) VALUES (?1, ?2, ?3)
                 ON CONFLICT(name) DO UPDATE SET steps_json = ?2, timestamp = ?3",
                params![name, steps_json, now_ts()],
            )
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Список сценариев.
    pub fn workflows(&self) -> ConvertResult<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, steps_json FROM workflows ORDER BY name")
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| ConvertError::Processing(e.to_string()))?);
        }
        Ok(result)
    }

    /// Удаляет сценарий.
    pub fn delete_workflow(&self, name: &str) -> ConvertResult<()> {
        self.conn
            .execute("DELETE FROM workflows WHERE name = ?1", params![name])
            .map_err(|e| ConvertError::Processing(e.to_string()))?;
        Ok(())
    }
}

fn row_to_queue(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueueEntry> {
    Ok(QueueEntry {
        id: row.get(0)?,
        task_type: row.get(1)?,
        payload_json: row.get(2)?,
        status: row.get(3)?,
        position: row.get(4)?,
        error: row.get(5)?,
        timestamp: row.get(6)?,
    })
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Стандартное расположение базы данных приложения.
pub fn default_db_path(app_dir: &Path) -> PathBuf {
    app_dir.join("convert.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_crud_and_search() {
        let db = Database::open_in_memory().unwrap();
        let now = now_ts();
        db.add_history(&HistoryEntry {
            id: None,
            operation: "PNG → WEBP".into(),
            tool: "image_studio".into(),
            source: "/imgs/photo.png".into(),
            output: Some("/imgs/photo.webp".into()),
            status: "success".into(),
            error: None,
            settings_json: Some(r#"{"quality":80}"#.into()),
            duration_ms: 120,
            timestamp: now,
        })
        .unwrap();
        db.add_history(&HistoryEntry {
            id: None,
            operation: "MP4 → MKV".into(),
            tool: "video_studio".into(),
            source: "/video/clip.mp4".into(),
            output: None,
            status: "error".into(),
            error: Some("кодек недоступен".into()),
            settings_json: None,
            duration_ms: 40,
            timestamp: now,
        })
        .unwrap();

        // Все записи.
        let all = db.history(None, None, 10).unwrap();
        assert_eq!(all.len(), 2);

        // Поиск.
        let found = db.history(Some("photo"), None, 10).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].operation, "PNG → WEBP");

        // Фильтр по статусу.
        let errors = db.history(None, Some("error"), 10).unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].error.as_deref().unwrap().contains("кодек"));

        // Удаление и очистка.
        db.delete_history(all[0].id.unwrap()).unwrap();
        assert_eq!(db.history(None, None, 10).unwrap().len(), 1);
        db.clear_history().unwrap();
        assert_eq!(db.history(None, None, 10).unwrap().len(), 0);
    }

    #[test]
    fn history_retention() {
        let db = Database::open_in_memory().unwrap();
        let old = now_ts() - 40 * 86_400; // 40 дней назад
        for i in 0..5 {
            db.add_history(&HistoryEntry {
                id: None,
                operation: format!("op {i}"),
                tool: "t".into(),
                source: "s".into(),
                output: None,
                status: "success".into(),
                error: None,
                settings_json: None,
                duration_ms: 1,
                timestamp: old,
            })
            .unwrap();
        }
        db.add_history(&HistoryEntry {
            id: None,
            operation: "новая".into(),
            tool: "t".into(),
            source: "s".into(),
            output: None,
            status: "success".into(),
            error: None,
            settings_json: None,
            duration_ms: 1,
            timestamp: now_ts(),
        })
        .unwrap();
        db.apply_history_retention(30, 0).unwrap();
        assert_eq!(db.history(None, None, 100).unwrap().len(), 1);

        db.apply_history_retention(0, 1).unwrap();
        assert_eq!(db.history(None, None, 100).unwrap().len(), 1);
    }

    #[test]
    fn queue_flow_and_recovery() {
        let db = Database::open_in_memory().unwrap();
        let id1 = db.enqueue("convert_image", r#"{"src":"a.png"}"#).unwrap();
        let id2 = db.enqueue("convert_video", r#"{"src":"b.mp4"}"#).unwrap();

        let next = db.dequeue_next().unwrap().unwrap();
        assert_eq!(next.id, Some(id1));
        assert_eq!(next.task_type, "convert_image");

        // Имитируем «падение» приложения: задача осталась running.
        // (dequeue_next уже поставил running)
        let recovered = db.recover_interrupted().unwrap();
        assert_eq!(recovered, 1);

        let next2 = db.dequeue_next().unwrap().unwrap();
        assert_eq!(next2.id, Some(id1)); // снова первая, позиция меньше
        db.set_queue_status(id1, "done", None).unwrap();
        let next3 = db.dequeue_next().unwrap().unwrap();
        assert_eq!(next3.id, Some(id2));
        db.set_queue_status(id2, "error", Some("упс")).unwrap();

        let all = db.queue_all().unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|q| q.status == "done"));
        assert!(all.iter().any(|q| q.status == "error"));

        db.clean_finished_queue().unwrap();
        let rest = db.queue_all().unwrap();
        assert_eq!(rest.len(), 1); // error остаётся для retry
    }

    #[test]
    fn settings_json_roundtrip() {
        let db = Database::open_in_memory().unwrap();
        let mut settings = convert_core::AppSettings::default();
        settings.appearance.theme = "dark".into();
        db.set_setting_json("app_settings", &settings).unwrap();

        let back: convert_core::AppSettings = db
            .get_setting_json("app_settings")
            .unwrap()
            .expect("настройки должны быть");
        assert_eq!(back.appearance.theme, "dark");

        // Обновление той же клавиши.
        settings.appearance.accent = "#FF0000".into();
        db.set_setting_json("app_settings", &settings).unwrap();
        let back2: convert_core::AppSettings = db.get_setting_json("app_settings").unwrap().unwrap();
        assert_eq!(back2.appearance.accent, "#FF0000");
    }

    #[test]
    fn presets_and_workflows() {
        let db = Database::open_in_memory().unwrap();
        db.save_preset("web-80", "image_studio", r#"{"quality":80}"#).unwrap();
        db.save_preset("web-90", "image_studio", r#"{"quality":90}"#).unwrap();
        db.save_preset("h264", "video_studio", r#"{"crf":23}"#).unwrap();

        let image_presets = db.presets("image_studio").unwrap();
        assert_eq!(image_presets.len(), 2);

        // upsert
        db.save_preset("web-80", "image_studio", r#"{"quality":85}"#).unwrap();
        assert_eq!(db.presets("image_studio").unwrap().len(), 2);

        db.delete_preset("web-90").unwrap();
        assert_eq!(db.presets("image_studio").unwrap().len(), 1);

        db.save_workflow(
            "фото-пайплайн",
            r#"[{"op":"compress"},{"op":"convert_webp"}]"#,
        )
        .unwrap();
        let wfs = db.workflows().unwrap();
        assert_eq!(wfs.len(), 1);
        assert!(wfs[0].1.contains("convert_webp"));
    }

    #[test]
    fn persistent_db_file() {
        let dir = std::env::temp_dir().join("convert_db_file");
        let _ = std::fs::remove_dir_all(&dir);
        let db_path = default_db_path(&dir);
        {
            let db = Database::open(&db_path).unwrap();
            db.set_setting("k", "v").unwrap();
        }
        {
            let db = Database::open(&db_path).unwrap();
            assert_eq!(db.get_setting("k").unwrap().as_deref(), Some("v"));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

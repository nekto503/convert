//! Автосценарии (ТЗ §5.5): слежение за папками.
//!
//! WatchService наблюдает за папкой; при появлении новых файлов
//! применяет действие (конвертация/сжатие/сценарий). По ТЗ обработка
//! запускается «после подтверждения пользователя»: watcher только
//! фиксирует события и отдаёт их UI через колбэк, постановка в очередь
//! происходит по подтверждению (или сразу, если правило с auto=true).
//!
//! Реальный тест watcher'а требует времени ожидания событий ОС;
//! в тестах проверяем логику правил и debounce.

use convert_core::error::ConvertResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Действие правила при появлении файла.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum WatchAction {
    /// Конвертировать изображения в формат.
    ConvertImages { format: String, quality: u8 },
    /// Сжать изображение (JPEG с качеством).
    CompressImage { quality: u8 },
    /// Запустить именованный сценарий (Workflow из БД).
    RunWorkflow { name: String },
}

/// Правило автосценария.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchRule {
    pub id: u64,
    pub name: String,
    /// Наблюдаемая папка.
    pub folder: PathBuf,
    /// Расширения-триггеры (пусто = любые).
    pub extensions: Vec<String>,
    pub action: WatchAction,
    /// Запускать без подтверждения.
    pub auto: bool,
    pub enabled: bool,
}

/// Событие: новый файл в наблюдаемой папке.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchEvent {
    pub rule_id: u64,
    pub path: PathBuf,
    /// Готовое предложение для UI.
    pub suggestion: String,
}

/// Диспетчер правил + debounce (события ОС дублируются).
pub struct WatchService {
    rules: Arc<Mutex<Vec<WatchRule>>>,
    events_tx: Sender<WatchEvent>,
    events_rx: Mutex<Receiver<WatchEvent>>,
    /// Время последнего события по пути (debounce).
    last_seen: Arc<Mutex<HashMap<PathBuf, Instant>>>,
}

impl WatchService {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Self {
            rules: Arc::new(Mutex::new(Vec::new())),
            events_tx: tx,
            events_rx: Mutex::new(rx),
            last_seen: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Все правила (для UI).
    pub fn rules(&self) -> Vec<WatchRule> {
        self.rules.lock().unwrap().clone()
    }

    /// Добавить/обновить правило (id присваивает вызывающий слой).
    pub fn upsert_rule(&self, rule: WatchRule) {
        let mut rules = self.rules.lock().unwrap();
        match rules.iter_mut().find(|r| r.id == rule.id) {
            Some(existing) => *existing = rule,
            None => rules.push(rule),
        }
    }

    /// Удалить правило.
    pub fn remove_rule(&self, id: u64) {
        self.rules.lock().unwrap().retain(|r| r.id != id);
    }

    /// Включить/выключить правило (ТЗ: возможность отключения).
    pub fn set_enabled(&self, id: u64, enabled: bool) {
        if let Some(r) = self.rules.lock().unwrap().iter_mut().find(|r| r.id == id) {
            r.enabled = enabled;
        }
    }

    /// Обработка события ФС: решает, подходит ли файл под правило,
    /// дебаунсит повторы. Вызывается потоком notify (в тестах — напрямую).
    pub fn handle_fs_event(&self, path: PathBuf) {
        if !path.is_file() {
            return;
        }
        // Файл мог ещё писаться; даём ОС мгновение (на практике notify
        // присылает несколько событий на один файл).
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        // Debounce: то же самое событие в течение 2 секунд — пропускаем.
        {
            let mut seen = self.last_seen.lock().unwrap();
            if let Some(t) = seen.get(&path) {
                if t.elapsed() < Duration::from_secs(2) {
                    return;
                }
            }
            seen.insert(path.clone(), Instant::now());
        }

        let rules = self.rules.lock().unwrap().clone();
        for rule in rules.iter().filter(|r| r.enabled) {
            let ext_ok = rule.extensions.is_empty()
                || rule.extensions.iter().any(|e| e.eq_ignore_ascii_case(&ext));
            if !ext_ok {
                continue;
            }
            if !path.starts_with(&rule.folder) {
                continue;
            }
            let suggestion = match &rule.action {
                WatchAction::ConvertImages { format, .. } => {
                    format!("«{}»: конвертировать {} в {format}?", rule.name, file_name(&path))
                }
                WatchAction::CompressImage { .. } => {
                    format!("«{}»: сжать {}?", rule.name, file_name(&path))
                }
                WatchAction::RunWorkflow { name } => {
                    format!("«{}»: запустить сценарий «{name}» для {}?", rule.name, file_name(&path))
                }
            };
            let _ = self.events_tx.send(WatchEvent {
                rule_id: rule.id,
                path: path.clone(),
                suggestion,
            });
        }
    }

    /// Очередь событий для UI (получатель забирает всё накопленное).
    pub fn drain_events(&self) -> Vec<WatchEvent> {
        let rx = self.events_rx.lock().unwrap();
        let mut out = Vec::new();
        while let Ok(e) = rx.try_recv() {
            out.push(e);
        }
        out
    }

    /// Запуск фонового наблюдателя (notify) для всех папок правил.
    /// Возвращает JoinHandle потока; в тестах не используется.
    pub fn spawn_observer(self: &Arc<Self>) -> std::thread::JoinHandle<()> {
        let service = Arc::clone(self);
        std::thread::spawn(move || {
            // Каждый интервал — пересоздаём watcher (правила меняются).
            loop {
                let folders: Vec<PathBuf> = {
                    let rules = service.rules.lock().unwrap();
                        rules
                            .iter()
                            .filter(|r| r.enabled)
                            .map(|r| r.folder.clone())
                            .collect()
                };
                if folders.is_empty() {
                    std::thread::sleep(Duration::from_secs(3));
                    continue;
                }
                let (tx, rx) = channel();
                use notify::Watcher;
                let mut watcher = match notify::recommended_watcher(move |res: Result<notify::Event, _>| {
                    if let Ok(event) = res {
                        for path in event.paths {
                            let _ = tx.send(path);
                        }
                    }
                }) {
                    Ok(w) => w,
                    Err(_) => {
                        std::thread::sleep(Duration::from_secs(5));
                        continue;
                    }
                };
                for folder in &folders {
                    let _ = watcher.watch(folder, notify::RecursiveMode::NonRecursive);
                }
                // Окно наблюдения 3 секунды, затем пересборка (новые правила).
                let deadline = Instant::now() + Duration::from_secs(3);
                while Instant::now() < deadline {
                    if let Ok(path) = rx.recv_timeout(Duration::from_millis(300)) {
                        service.handle_fs_event(path);
                    }
                }
            }
        })
    }
}

fn file_name(path: &PathBuf) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Формирует задачу для очереди по событию и правилу (после подтверждения).
pub fn task_for_event(event: &WatchEvent, rule: &WatchRule) -> ConvertResult<convert_pipeline::TaskKind> {
    let _ = event;
    match &rule.action {
        WatchAction::ConvertImages { format, quality } => {
            let target = match format.to_lowercase().as_str() {
                "png" => convert_image::ImageOutputFormat::Png,
                "jpg" | "jpeg" => convert_image::ImageOutputFormat::Jpeg,
                "bmp" => convert_image::ImageOutputFormat::Bmp,
                "tiff" => convert_image::ImageOutputFormat::Tiff,
                _ => convert_image::ImageOutputFormat::Png,
            };
            let job = convert_image::ImageJob {
                source: event.path.clone(),
                target_format: target,
                encode: convert_image::EncodeParams { quality: *quality, dpi: None },
                ..Default::default()
            };
            Ok(convert_pipeline::TaskKind::ImageJob {
                payload_json: serde_json::to_string(&job)
                    .map_err(|e| convert_core::ConvertError::Processing(e.to_string()))?,
            })
        }
        WatchAction::CompressImage { quality } => {
            let job = convert_image::ImageJob {
                source: event.path.clone(),
                target_format: convert_image::ImageOutputFormat::Jpeg,
                encode: convert_image::EncodeParams { quality: *quality, dpi: None },
                suffix: Some("_compressed".into()),
                ..Default::default()
            };
            Ok(convert_pipeline::TaskKind::ImageJob {
                payload_json: serde_json::to_string(&job)
                    .map_err(|e| convert_core::ConvertError::Processing(e.to_string()))?,
            })
        }
        WatchAction::RunWorkflow { name } => Ok(convert_pipeline::TaskKind::Generic {
            operation: "workflow_run".into(),
            payload_json: serde_json::json!({
                "workflow_name": name,
                "inputs": [event.path]
            })
            .to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: u64, folder: &std::path::Path, exts: &[&str]) -> WatchRule {
        WatchRule {
            id,
            name: format!("правило {id}"),
            folder: folder.to_path_buf(),
            extensions: exts.iter().map(|s| s.to_string()).collect(),
            action: WatchAction::CompressImage { quality: 80 },
            auto: false,
            enabled: true,
        }
    }

    #[test]
    fn matching_file_produces_event() {
        let dir = std::env::temp_dir().join("convert_watch1");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let service = Arc::new(WatchService::new());
        service.upsert_rule(rule(1, &dir, &["png", "jpg"]));

        let file = dir.join("photo.png");
        std::fs::write(&file, b"png").unwrap();
        service.handle_fs_event(file);

        let events = service.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].rule_id, 1);
        assert!(events[0].suggestion.contains("сжать"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_matching_extension_ignored() {
        let dir = std::env::temp_dir().join("convert_watch2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let service = Arc::new(WatchService::new());
        service.upsert_rule(rule(1, &dir, &["png"]));

        let file = dir.join("doc.txt");
        std::fs::write(&file, b"text").unwrap();
        service.handle_fs_event(file);
        assert!(service.drain_events().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disabled_rule_ignored() {
        let dir = std::env::temp_dir().join("convert_watch3");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let service = Arc::new(WatchService::new());
        service.upsert_rule(rule(1, &dir, &[]));
        service.set_enabled(1, false);

        let file = dir.join("x.png");
        std::fs::write(&file, b"png").unwrap();
        service.handle_fs_event(file);
        assert!(service.drain_events().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn debounce_deduplicates() {
        let dir = std::env::temp_dir().join("convert_watch4");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let service = Arc::new(WatchService::new());
        service.upsert_rule(rule(1, &dir, &[]));

        let file = dir.join("y.png");
        std::fs::write(&file, b"png").unwrap();
        service.handle_fs_event(file.clone());
        service.handle_fs_event(file.clone()); // дубль события ОС
        service.handle_fs_event(file.clone());
        let events = service.drain_events();
        assert_eq!(events.len(), 1, "дубликаты должны схлопнуться");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_outside_rule_ignored() {
        let dir = std::env::temp_dir().join("convert_watch5");
        let other = std::env::temp_dir().join("convert_watch5_other");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&other);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let service = Arc::new(WatchService::new());
        service.upsert_rule(rule(1, &dir, &[]));

        let file = other.join("z.png");
        std::fs::write(&file, b"png").unwrap();
        service.handle_fs_event(file);
        assert!(service.drain_events().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn remove_rule_works() {
        let service = WatchService::new();
        let dir = std::env::temp_dir();
        service.upsert_rule(rule(1, &dir, &[]));
        assert_eq!(service.rules().len(), 1);
        service.remove_rule(1);
        assert!(service.rules().is_empty());
    }

    #[test]
    fn task_for_event_builds_image_job() {
        let dir = std::env::temp_dir().join("convert_watch6");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let service = Arc::new(WatchService::new());
        let r = rule(1, &dir, &[]);
        service.upsert_rule(r.clone());

        let file = dir.join("a.png");
        std::fs::write(&file, b"png").unwrap();
        service.handle_fs_event(file);
        let event = &service.drain_events()[0];

        let task = task_for_event(event, &r).unwrap();
        match task {
            convert_pipeline::TaskKind::ImageJob { payload_json } => {
                let job: convert_image::ImageJob = serde_json::from_str(&payload_json).unwrap();
                assert!(job.source.ends_with("a.png"));
                assert!(job.suffix.as_deref().unwrap().contains("compressed"));
            }
            other => panic!("ожидалась ImageJob, получено {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

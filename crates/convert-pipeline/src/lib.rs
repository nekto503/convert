//! convert-pipeline — система пакетной обработки (ТЗ раздел 6).
//!
//! Архитектура:
//! - `TaskKind` — тип задачи; исполнитель задаётся замыканием
//!   `TaskRunner` (в Tauri-слое замыкание вызывает нужный движок);
//! - `BatchQueue` — общая очередь с приоритетами: задачи добавляются,
//!   параллельные воркеры разбирают их, прогресс идёт событиями;
//! - пауза/продолжение/отмена/повтор ошибки; результаты пишутся
//!   в историю (convert-db) самим исполнителем.
//!
//! Потокобезопасно: `Arc<Mutex<...>>` вокруг состояния очереди.

use convert_core::error::{ConvertError, ConvertResult};
use serde::{Deserialize, Serialize};
use std::collections::BinaryHeap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Тип задачи пакетной обработки.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "payload")]
pub enum TaskKind {
    /// Конвертация/обработка изображения (ImageJob из convert-image).
    ImageJob { payload_json: String },
    /// Видеозадача (VideoJob из convert-media).
    VideoJob { payload_json: String },
    /// Аудиозадача (AudioJob).
    AudioJob { payload_json: String },
    /// Офисный документ → другой формат.
    OfficeConvert { source: PathBuf, target: String, output_dir: Option<PathBuf> },
    /// Произвольная операция с одним файлом (обрезка PDF, OCR...).
    Generic { operation: String, payload_json: String },
}

impl TaskKind {
    pub fn label(&self) -> String {
        match self {
            Self::ImageJob { .. } => "Изображение".into(),
            Self::VideoJob { .. } => "Видео".into(),
            Self::AudioJob { .. } => "Аудио".into(),
            Self::OfficeConvert { .. } => "Документ".into(),
            Self::Generic { operation, .. } => operation.clone(),
        }
    }
}

/// Приоритет: меньше число — раньше исполнение.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    High = 0,
    Normal = 1,
    Low = 2,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Состояние задачи.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Done,
    Error,
    Cancelled,
}

/// Задача в очереди.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub kind: TaskKind,
    pub priority: Priority,
    pub status: TaskStatus,
    pub error: Option<String>,
    /// Результат (путь) для «открыть папку результата».
    pub output: Option<PathBuf>,
}

/// Результат выполнения задачи от исполнителя.
#[derive(Debug, Clone)]
pub struct TaskOutput {
    pub output_path: Option<PathBuf>,
}

/// Исполнитель задач: замыкание, вызывающее движок.
/// Возвращает прогресс-канал неявно через ProgressCallback.
pub type TaskRunner = Arc<dyn Fn(&Task, &ProgressCallback) -> ConvertResult<TaskOutput> + Send + Sync>;

/// Колбэк прогресса (0..=100) — пробрасывается в UI.
pub type ProgressCallback = Arc<dyn Fn(u8) + Send + Sync>;

/// Событие очереди для подписчиков (UI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueueEvent {
    TaskStarted { id: u64 },
    TaskProgress { id: u64, percent: u8 },
    TaskFinished { id: u64, output: Option<PathBuf> },
    TaskFailed { id: u64, error: String },
    TaskCancelled { id: u64 },
    QueueEmpty,
    AllDone { done: u64, failed: u64 },
}

/// Элемент кучи с приоритетом.
#[derive(Debug)]
struct HeapItem {
    priority: Priority,
    seq: u64,
    task: Task,
}

impl PartialEq for HeapItem {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.seq == other.seq
    }
}
impl Eq for HeapItem {}
impl PartialOrd for HeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // BinaryHeap — max-heap: меньший приоритет = выше.
        (self.priority as u8, self.seq)
            .cmp(&(other.priority as u8, other.seq))
            .reverse()
    }
}

struct QueueState {
    heap: BinaryHeap<HeapItem>,
    running: Vec<u64>,
    finished: Vec<Task>,
    paused: bool,
    cancelled_all: bool,
}

/// Пул задач пакетной обработки.
pub struct BatchQueue {
    state: Mutex<QueueState>,
    signal: Condvar,
    next_id: AtomicU64,
    runner: TaskRunner,
    workers: Mutex<Vec<std::thread::JoinHandle<()>>>,
    max_workers: usize,
}

impl BatchQueue {
    /// Создаёт очередь с исполнителем и числом воркеров.
    ///
    /// `max_workers = 0` — авто: число ядер CPU.
    pub fn new(runner: TaskRunner, max_workers: usize) -> Arc<Self> {
        let workers = if max_workers == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get().clamp(1, 8))
                .unwrap_or(2)
        } else {
            max_workers.clamp(1, 32)
        };
        Arc::new(Self {
            state: Mutex::new(QueueState {
                heap: BinaryHeap::new(),
                running: Vec::new(),
                finished: Vec::new(),
                paused: false,
                cancelled_all: false,
            }),
            signal: Condvar::new(),
            next_id: AtomicU64::new(1),
            runner,
            workers: Mutex::new(Vec::new()),
            max_workers: workers,
        })
    }

    /// Добавляет задачу; возвращает её id.
    pub fn push(self: &Arc<Self>, kind: TaskKind, priority: Priority) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let task = Task {
            id,
            kind,
            priority,
            status: TaskStatus::Pending,
            error: None,
            output: None,
        };
        let mut state = self.state.lock().unwrap();
        state.heap.push(HeapItem { priority, seq: id, task });
        drop(state);
        self.signal.notify_one();
        id
    }

    /// Запускает воркеры (один раз).
    pub fn start(self: &Arc<Self>) {
        let mut workers = self.workers.lock().unwrap();
        if !workers.is_empty() {
            return;
        }
        for i in 0..self.max_workers {
            let queue = Arc::clone(self);
            let handle = std::thread::Builder::new()
                .name(format!("convert-worker-{i}"))
                .spawn(move || queue.worker_loop())
                .expect("не удалось создать воркер");
            workers.push(handle);
        }
    }

    /// Пауза: воркеры не берут новые задачи.
    pub fn pause(&self) {
        self.state.lock().unwrap().paused = true;
    }

    /// Продолжение.
    pub fn resume(&self) {
        let mut state = self.state.lock().unwrap();
        state.paused = false;
        drop(state);
        self.signal.notify_all();
    }

    /// Отмена одной задачи (если ждёт в очереди).
    pub fn cancel(&self, id: u64) -> bool {
        let mut state = self.state.lock().unwrap();
        let before = state.heap.len();
        let mut found = false;
        let mut kept = BinaryHeap::new();
        while let Some(item) = state.heap.pop() {
            if item.task.id == id {
                let mut t = item.task;
                t.status = TaskStatus::Cancelled;
                state.finished.push(t);
                found = true;
            } else {
                kept.push(item);
            }
        }
        state.heap = kept;
        let _ = before;
        found
    }

    /// Отмена всех ожидающих задач.
    pub fn cancel_all(&self) -> usize {
        let mut state = self.state.lock().unwrap();
        let mut n = 0;
        while let Some(item) = state.heap.pop() {
            let mut t = item.task;
            t.status = TaskStatus::Cancelled;
            state.finished.push(t);
            n += 1;
        }
        n
    }

    /// Снимок задач для UI.
    pub fn snapshot(&self) -> Vec<Task> {
        let state = self.state.lock().unwrap();
        let mut result: Vec<Task> = state.heap.iter().map(|h| h.task.clone()).collect();
        result.extend(state.finished.iter().cloned());
        drop(state);
        result.sort_by_key(|t| t.id);
        result
    }

    /// Повтор неудачной задачи: возвращает новую копию в очередь.
    pub fn retry(self: &Arc<Self>, id: u64) -> Option<u64> {
        let task = {
            let state = self.state.lock().unwrap();
            state
                .finished
                .iter()
                .find(|t| t.id == id)
                .filter(|t| t.status == TaskStatus::Error)
                .cloned()
        }?;
        Some(self.push(task.kind, task.priority))
    }

    /// Ждёт завершения всех задач (для тестов и shutdown).
    pub fn wait_all(&self, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            {
                let state = self.state.lock().unwrap();
                if state.heap.is_empty() && state.running.is_empty() {
                    return true;
                }
            }
            if std::time::Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn worker_loop(self: Arc<Self>) {
        loop {
            let task = {
                let mut state = self.state.lock().unwrap();
                loop {
                    if state.paused {
                        // Ждём снятия паузы.
                        state = self.signal.wait(state).unwrap();
                        continue;
                    }
                    match state.heap.pop() {
                        Some(item) => {
                            let mut task = item.task;
                            task.status = TaskStatus::Running;
                            state.running.push(task.id);
                            break task;
                        }
                        None => {
                            // Очередь пуста: ждём новых задач с таймаутом.
                            let (guard, _timeout) = self
                                .signal
                                .wait_timeout(state, Duration::from_secs(1))
                                .unwrap();
                            state = guard;
                        }
                    }
                }
            };

            // Исполнение вне блокировки очереди.
            let progress: ProgressCallback = Arc::new(|_p| {
                // Прогресс пробрасывается в Tauri-события на уровне оболочки.
            });
            let result = (self.runner)(&task, &progress);

            let mut state = self.state.lock().unwrap();
            state.running.retain(|&id| id != task.id);
            let mut finished = task.clone();
            match result {
                Ok(out) => {
                    finished.status = TaskStatus::Done;
                    finished.output = out.output_path;
                }
                Err(ConvertError::Cancelled) => {
                    finished.status = TaskStatus::Cancelled;
                }
                Err(e) => {
                    finished.status = TaskStatus::Error;
                    finished.error = Some(e.to_string());
                }
            }
            state.finished.push(finished);
            let empty = state.heap.is_empty() && state.running.is_empty();
            drop(state);
            if empty {
                // Пусть проснутся воркеры на случай новых задач.
                self.signal.notify_all();
            }
        }
    }
}

/// Текущая метка времени Unix.
pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Запись результата в историю (вызывается оболочкой после задачи).
pub fn record_history(
    db: &convert_db::Database,
    task: &Task,
    started_ts: i64,
) -> ConvertResult<i64> {
    db.add_history(&convert_db::HistoryEntry {
        id: None,
        operation: task.kind.label(),
        tool: tool_of(&task.kind),
        source: source_of(&task.kind),
        output: task.output.as_ref().map(|p| p.display().to_string()),
        status: match task.status {
            TaskStatus::Done => "success".to_string(),
            TaskStatus::Cancelled => "cancelled".to_string(),
            _ => "error".to_string(),
        },
        error: task.error.clone(),
        settings_json: Some(serde_json::to_string(&task.kind).unwrap_or_default()),
        duration_ms: (now_ts() - started_ts).max(0) as u64 * 1000,
        timestamp: now_ts(),
    })
}

fn tool_of(kind: &TaskKind) -> String {
    match kind {
        TaskKind::ImageJob { .. } => "image_studio".into(),
        TaskKind::VideoJob { .. } => "video_studio".into(),
        TaskKind::AudioJob { .. } => "audio_studio".into(),
        TaskKind::OfficeConvert { .. } => "document_studio".into(),
        TaskKind::Generic { .. } => "tools".into(),
    }
}

fn source_of(kind: &TaskKind) -> String {
    // Источник извлекается из JSON-полей задач.
    match kind {
        TaskKind::ImageJob { payload_json } => extract_source(payload_json),
        TaskKind::VideoJob { payload_json } => extract_source(payload_json),
        TaskKind::AudioJob { payload_json } => extract_source(payload_json),
        TaskKind::OfficeConvert { source, .. } => source.display().to_string(),
        TaskKind::Generic { payload_json, .. } => extract_source(payload_json),
    }
}

fn extract_source(json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| {
            v.get("source")
                .or_else(|| v.get("src"))
                .and_then(|s| s.as_str())
                .map(String::from)
        })
        .unwrap_or_else(|| "(неизвестно)".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn noop_runner() -> TaskRunner {
        Arc::new(|_task, _progress| {
            std::thread::sleep(Duration::from_millis(20));
            Ok(TaskOutput { output_path: None })
        })
    }

    #[test]
    fn executes_tasks_in_parallel() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c2 = Arc::clone(&counter);
        let runner: TaskRunner = Arc::new(move |_t, _p| {
            c2.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(150));
            Ok(TaskOutput { output_path: None })
        });
        let queue = BatchQueue::new(runner, 4);
        queue.start();
        for _ in 0..8 {
            queue.push(
                TaskKind::Generic { operation: "test".into(), payload_json: "{}".into() },
                Priority::Normal,
            );
        }
        assert!(queue.wait_all(Duration::from_secs(10)), "задачи не завершились");
        assert_eq!(counter.load(Ordering::SeqCst), 8);
        let snapshot = queue.snapshot();
        assert!(snapshot.iter().all(|t| t.status == TaskStatus::Done));
    }

    #[test]
    fn priority_order_respected() {
        // Один воркер: High должен уйти раньше Low (при равной позиции).
        let order = Arc::new(Mutex::new(Vec::new()));
        let o2 = Arc::clone(&order);
        let runner: TaskRunner = Arc::new(move |task: &Task, _p| {
            o2.lock().unwrap().push(task.id);
            Ok(TaskOutput { output_path: None })
        });
        let queue = BatchQueue::new(runner, 1);
        let low = queue.push(
            TaskKind::Generic { operation: "low".into(), payload_json: "{}".into() },
            Priority::Low,
        );
        let high = queue.push(
            TaskKind::Generic { operation: "high".into(), payload_json: "{}".into() },
            Priority::High,
        );
        queue.start();
        assert!(queue.wait_all(Duration::from_secs(5)));
        let order = order.lock().unwrap().clone();
        assert_eq!(order, vec![high, low], "High должен исполниться первым");
    }

    #[test]
    fn error_captured_and_retryable() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let a2 = Arc::clone(&attempts);
        let runner: TaskRunner = Arc::new(move |task: &Task, _p| {
            if a2.fetch_add(1, Ordering::SeqCst) == 0 && task.id == 1 {
                return Err(ConvertError::Processing("первый раз падаем".into()));
            }
            Ok(TaskOutput { output_path: Some(PathBuf::from("/tmp/result.png")) })
        });
        let queue = BatchQueue::new(runner, 1);
        queue.start();
        let id = queue.push(
            TaskKind::Generic { operation: "flaky".into(), payload_json: "{}".into() },
            Priority::Normal,
        );
        assert!(queue.wait_all(Duration::from_secs(5)));

        let snap = queue.snapshot();
        let failed = snap.iter().find(|t| t.id == id).unwrap();
        assert_eq!(failed.status, TaskStatus::Error);
        assert!(failed.error.as_deref().unwrap().contains("падаем"));

        // Повтор.
        let retry_id = queue.retry(id).expect("retry обязан работать для error");
        assert!(queue.wait_all(Duration::from_secs(5)));
        let snap2 = queue.snapshot();
        let retried = snap2.iter().find(|t| t.id == retry_id).unwrap();
        assert_eq!(retried.status, TaskStatus::Done);
        assert_eq!(retried.output, Some(PathBuf::from("/tmp/result.png")));
    }

    #[test]
    fn cancel_pending_task() {
        let runner: TaskRunner = Arc::new(|_t, _p| {
            std::thread::sleep(Duration::from_millis(100));
            Ok(TaskOutput { output_path: None })
        });
        let queue = BatchQueue::new(runner, 1); // один воркер
        queue.start();
        let first = queue.push(
            TaskKind::Generic { operation: "long".into(), payload_json: "{}".into() },
            Priority::Normal,
        );
        let second = queue.push(
            TaskKind::Generic { operation: "will-cancel".into(), payload_json: "{}".into() },
            Priority::Normal,
        );
        // Пока первый выполняется, отменяем второй.
        std::thread::sleep(Duration::from_millis(20));
        assert!(queue.cancel(second));
        assert!(queue.wait_all(Duration::from_secs(5)));

        let snap = queue.snapshot();
        assert_eq!(
            snap.iter().find(|t| t.id == first).unwrap().status,
            TaskStatus::Done
        );
        assert_eq!(
            snap.iter().find(|t| t.id == second).unwrap().status,
            TaskStatus::Cancelled
        );
    }

    #[test]
    fn pause_and_resume() {
        let runner: TaskRunner = Arc::new(|_t, _p| {
            std::thread::sleep(Duration::from_millis(30));
            Ok(TaskOutput { output_path: None })
        });
        let queue = BatchQueue::new(runner, 1);
        queue.start();
        for _ in 0..3 {
            queue.push(
                TaskKind::Generic { operation: "p".into(), payload_json: "{}".into() },
                Priority::Normal,
            );
        }
        queue.pause();
        std::thread::sleep(Duration::from_millis(200));
        // В паузе выполнена максимум одна (взятая до паузы) задача.
        let done_in_pause = queue
            .snapshot()
            .iter()
            .filter(|t| t.status == TaskStatus::Done)
            .count();
        assert!(done_in_pause <= 1, "в паузе выполнено {done_in_pause}");

        queue.resume();
        assert!(queue.wait_all(Duration::from_secs(5)));
        assert_eq!(
            queue.snapshot().iter().filter(|t| t.status == TaskStatus::Done).count(),
            3
        );
    }

    #[test]
    fn history_recorded() {
        let db = convert_db::Database::open_in_memory().unwrap();
        let task = Task {
            id: 7,
            kind: TaskKind::ImageJob { payload_json: r#"{"source":"/x/a.png"}"#.into() },
            priority: Priority::Normal,
            status: TaskStatus::Done,
            error: None,
            output: Some(PathBuf::from("/x/a.webp")),
        };
        record_history(&db, &task, now_ts()).unwrap();
        let entries = db.history(None, None, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, "/x/a.png");
        assert_eq!(entries[0].output.as_deref(), Some("/x/a.webp"));
        assert_eq!(entries[0].tool, "image_studio");
    }
}

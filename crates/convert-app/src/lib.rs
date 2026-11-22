//! convert-app — слой команд приложения (то, что Tauri вызывает из UI).
//!
//! Этот крейт не зависит от tauri: вся логика команд описана здесь и
//! покрыта тестами. Оболочка src-tauri лишь регистрирует функции как
//! #[tauri::command] и делегирует сюда — гарантируем, что поведение
//! команд проверяемо без GUI.
//!
//! `AppContext` — общее состояние: база данных, очередь, настройки.
//! В Tauri оно оборачивается в tauri::State.

pub mod commands;
pub mod context;
pub mod runner;
pub mod watch;
pub mod workflows;

pub use context::AppContext;
pub use runner::build_runner;
pub use watch::{WatchAction, WatchEvent, WatchRule, WatchService};
pub use workflows::{run_workflow, Workflow, WorkflowRunReport, WorkflowStep};

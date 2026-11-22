//! Библиотечная часть Tauri-оболочки CONVERT.
//!
//! Вся логика команд живёт в крейте convert-app (протестирована);
//! здесь — только регистрация #[tauri::command] обёрток, tray
//! и управление окном.

use convert_app::AppContext;
use convert_pipeline::{Priority, Task, TaskKind};
use std::sync::Arc;
use tauri::{Manager, State, TrayIconBuilder};

pub struct Ctx {
    pub app: Arc<AppContext>,
    pub watcher: Arc<convert_app::WatchService>,
}

fn err_to_string<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ---- Smart Scan ------------------------------------------------------------

#[tauri::command]
fn scan_file(path: String) -> Result<serde_json::Value, String> {
    convert_app::commands::scan_file(&path).map_err(err_to_string)
}

// ---- Очередь ----------------------------------------------------------------

#[tauri::command]
fn enqueue(ctx: State<Ctx>, kind: TaskKind, priority: Option<Priority>) -> u64 {
    convert_app::commands::enqueue(&ctx.app, kind, priority.unwrap_or(Priority::Normal))
}

#[tauri::command]
fn queue_snapshot(ctx: State<Ctx>) -> Vec<Task> {
    convert_app::commands::queue_snapshot(&ctx.app)
}

#[tauri::command]
fn queue_pause(ctx: State<Ctx>) {
    convert_app::commands::queue_pause(&ctx.app)
}

#[tauri::command]
fn queue_resume(ctx: State<Ctx>) {
    convert_app::commands::queue_resume(&ctx.app)
}

#[tauri::command]
fn queue_cancel(ctx: State<Ctx>, id: u64) -> bool {
    convert_app::commands::queue_cancel(&ctx.app, id)
}

#[tauri::command]
fn queue_cancel_all(ctx: State<Ctx>) -> usize {
    convert_app::commands::queue_cancel_all(&ctx.app)
}

#[tauri::command]
fn queue_retry(ctx: State<Ctx>, id: u64) -> Option<u64> {
    convert_app::commands::queue_retry(&ctx.app, id)
}

// ---- История ------------------------------------------------------------------

#[tauri::command]
fn history(
    ctx: State<Ctx>,
    query: Option<String>,
    status: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<convert_db::HistoryEntry>, String> {
    convert_app::commands::history(&ctx.app, query, status, limit.unwrap_or(100))
        .map_err(err_to_string)
}

#[tauri::command]
fn history_delete(ctx: State<Ctx>, id: i64) -> Result<(), String> {
    convert_app::commands::history_delete(&ctx.app, id).map_err(err_to_string)
}

#[tauri::command]
fn history_clear(ctx: State<Ctx>) -> Result<(), String> {
    convert_app::commands::history_clear(&ctx.app).map_err(err_to_string)
}

// ---- Настройки ------------------------------------------------------------------

#[tauri::command]
fn get_settings(ctx: State<Ctx>) -> convert_core::AppSettings {
    convert_app::commands::get_settings(&ctx.app)
}

#[tauri::command]
fn save_settings(ctx: State<Ctx>, settings: convert_core::AppSettings) -> Result<(), String> {
    convert_app::commands::save_settings(&ctx.app, settings).map_err(err_to_string)
}

// ---- Компоненты и инструменты ------------------------------------------------------

#[tauri::command]
fn engines_status() -> Vec<convert_media::EngineStatus> {
    convert_app::commands::engines_status()
}

#[tauri::command]
fn image_metadata(path: String) -> Result<convert_image::ImageMetadata, String> {
    convert_app::commands::image_metadata(&path).map_err(err_to_string)
}

#[tauri::command]
fn image_palette(path: String, max_colors: Option<usize>) -> Result<Vec<convert_image::PaletteColor>, String> {
    convert_app::commands::image_palette(&path, max_colors.unwrap_or(8)).map_err(err_to_string)
}

#[tauri::command]
fn smart_compress_analyze(
    path: String,
    goal: Option<String>,
) -> Result<convert_tools::SmartCompressPlan, String> {
    let goal = match goal.as_deref() {
        Some("smallest") => convert_tools::CompressGoal::SmallestSize,
        Some("quality") => convert_tools::CompressGoal::Quality,
        _ => convert_tools::CompressGoal::Balanced,
    };
    convert_app::commands::smart_compress_analyze(&path, goal).map_err(err_to_string)
}

#[tauri::command]
fn find_duplicates(root: String) -> Result<Vec<convert_tools::DuplicateGroup>, String> {
    convert_app::commands::find_duplicates(&root).map_err(err_to_string)
}

#[tauri::command]
fn archive_list(path: String) -> Result<Vec<convert_archive::ArchiveEntry>, String> {
    convert_app::commands::archive_list(&path).map_err(err_to_string)
}

#[tauri::command]
fn pdf_metadata(path: String) -> Result<convert_pdf::PdfMetadata, String> {
    convert_app::commands::pdf_metadata(&path).map_err(err_to_string)
}

#[tauri::command]
fn pdf_text(path: String) -> Result<String, String> {
    convert_app::commands::pdf_text(&path).map_err(err_to_string)
}

#[tauri::command]
fn ocr_image(path: String, langs: Option<String>) -> Result<convert_media::OcrOutput, String> {
    convert_app::commands::ocr_image(&path, langs.as_deref().unwrap_or("eng")).map_err(err_to_string)
}

#[tauri::command]
fn probe_media(path: String) -> Result<convert_media::MediaInfo, String> {
    convert_app::commands::probe_media(&path).map_err(err_to_string)
}

// ---- Пресеты и сценарии ---------------------------------------------------------

#[tauri::command]
fn presets_list(ctx: State<Ctx>, tool: String) -> Result<Vec<(String, String)>, String> {
    convert_app::commands::presets_list(&ctx.app, &tool).map_err(err_to_string)
}

#[tauri::command]
fn preset_save(ctx: State<Ctx>, name: String, tool: String, settings_json: String) -> Result<i64, String> {
    convert_app::commands::preset_save(&ctx.app, &name, &tool, &settings_json).map_err(err_to_string)
}

#[tauri::command]
fn preset_delete(ctx: State<Ctx>, name: String) -> Result<(), String> {
    convert_app::commands::preset_delete(&ctx.app, &name).map_err(err_to_string)
}

#[tauri::command]
fn workflows_list(ctx: State<Ctx>) -> Result<Vec<(String, String)>, String> {
    convert_app::commands::workflows_list(&ctx.app).map_err(err_to_string)
}

#[tauri::command]
fn workflow_save(ctx: State<Ctx>, name: String, steps_json: String) -> Result<i64, String> {
    convert_app::commands::workflow_save(&ctx.app, &name, &steps_json).map_err(err_to_string)
}

#[tauri::command]
fn workflow_delete(ctx: State<Ctx>, name: String) -> Result<(), String> {
    convert_app::commands::workflow_delete(&ctx.app, &name).map_err(err_to_string)
}

// ---- Диалоги -------------------------------------------------------------------

#[tauri::command]
async fn pick_files() -> Result<Vec<String>, String> {
    // Диалоги файлов реализуются через tauri-plugin-dialog; чтобы не
    // тянуть плагин в каркас, UI использует drag&drop и стандартный
    // <input type="file">. Команда оставлена для полноты API.
    Err("используйте drag & drop или <input type=file>".into())
}

#[tauri::command]
fn show_in_folder(path: String) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .args(["/select,", &path])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(
            std::path::Path::new(&path).parent().unwrap_or(std::path::Path::new(".")),
        ).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").args(["-R", &path]).spawn();
    }
}


// ---- File Compare -----------------------------------------------------------

#[tauri::command]
fn compare_files(left: String, right: String) -> Result<convert_tools::CompareReport, String> {
    convert_app::commands::compare_files(&left, &right).map_err(err_to_string)
}

// ---- AI File Assistant (локальный rule-based) ----------------------------------

#[tauri::command]
fn assistant_ask(query: String, files: Option<Vec<String>>) -> convert_tools::AssistantReply {
    convert_app::commands::assistant_ask(&query, files.as_deref().unwrap_or(&[]))
}

// ---- Quality Enhancer ------------------------------------------------------------

#[tauri::command]
fn enhance_image(path: String, output: Option<String>, level: Option<String>) -> Result<convert_tools::EnhanceResult, String> {
    let output = output.unwrap_or_else(|| {
        std::path::Path::new(&path).with_extension("enhanced.png").to_string_lossy().to_string()
    });
    convert_app::commands::enhance_image(&path, &output, level.as_deref().unwrap_or("medium"))
        .map_err(err_to_string)
}

// ---- Workflow Builder ---------------------------------------------------------------

#[tauri::command]
fn workflow_run(ctx: State<Ctx>, name: String, inputs: Vec<String>) -> Result<convert_app::WorkflowRunReport, String> {
    convert_app::commands::workflow_run(&ctx.app, &name, &inputs).map_err(err_to_string)
}

// ---- Автосценарии (watch) ---------------------------------------------------------------

#[tauri::command]
fn watch_rules(ctx: State<Ctx>) -> Vec<convert_app::WatchRule> {
    convert_app::commands::watch_rules(&ctx.watcher)
}

#[tauri::command]
fn watch_upsert(ctx: State<Ctx>, rule: convert_app::WatchRule) {
    convert_app::commands::watch_upsert(&ctx.watcher, rule);
}

#[tauri::command]
fn watch_remove(ctx: State<Ctx>, id: u64) {
    convert_app::commands::watch_remove(&ctx.watcher, id);
}

#[tauri::command]
fn watch_set_enabled(ctx: State<Ctx>, id: u64, enabled: bool) {
    convert_app::commands::watch_set_enabled(&ctx.watcher, id, enabled);
}

#[tauri::command]
fn watch_events(ctx: State<Ctx>) -> Vec<convert_app::WatchEvent> {
    convert_app::commands::watch_events(&ctx.watcher)
}

/// Подтверждение автосценария: поставить задачу по событию.
#[tauri::command]
fn watch_confirm(ctx: State<Ctx>, event: convert_app::WatchEvent) -> Result<u64, String> {
    let rules = ctx.watcher.rules();
    let rule = rules
        .into_iter()
        .find(|r| r.id == event.rule_id)
        .ok_or_else(|| "правило не найдено".to_string())?;
    let task = convert_app::watch::task_for_event(&event, &rule).map_err(err_to_string)?;
    Ok(ctx.app.queue.push(task, Priority::Low))
}


// ---- Metadata Editor (ТЗ §4.10) ---------------------------------------------------

#[tauri::command]
fn image_strip_metadata(path: String) -> Result<(), String> {
    convert_image::strip_image_metadata(std::path::Path::new(&path)).map_err(err_to_string)
}

#[tauri::command]
fn image_metadata_edit(
    path: String,
    artist: Option<String>,
    copyright: Option<String>,
    description: Option<String>,
    strip_personal: Option<bool>,
) -> Result<(), String> {
    let edit = convert_image::ExifEdit {
        artist,
        copyright,
        image_description: description,
        strip_personal: strip_personal.unwrap_or(false),
    };
    convert_image::exif_edit(std::path::Path::new(&path), &edit).map_err(err_to_string)
}

// ---- Запуск при открытии файла через «Открыть с помощью» -----------------------------

/// Обработка файла, переданного при старте (ассоциации/Open With).
#[tauri::command]
fn startup_file() -> Option<String> {
    STARTUP_FILE.with(|f| f.borrow().clone())
}

thread_local! {
    static STARTUP_FILE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Файл, открытый через «Открыть с помощью»/ассоциацию (первый аргумент).
    if let Some(path) = std::env::args().nth(1) {
        STARTUP_FILE.with(|f| *f.borrow_mut() = Some(path));
    }

    // Двухфазная инициализация: контекст → runner над ним → очередь.
    let data_dir = app_data_dir();
    let ctx = std::sync::Arc::new(
        AppContext::new(data_dir.clone(), {
            Arc::new(|_t, _p| Ok(convert_pipeline::TaskOutput { output_path: None }))
        }, 0)
        .expect("не удалось создать контекст приложения"),
    );
    let runner = convert_app::runner::build_runner(Arc::clone(&ctx));
    let final_ctx = AppContext::new(data_dir, runner, 0).expect("контекст приложения");

    // Watch-сервис автосценариев.
    let watcher = Arc::new(convert_app::WatchService::new());
    watcher.spawn_observer();

    tauri::Builder::default()
        .manage(Ctx { app: Arc::new(final_ctx), watcher })
        .setup(|app| {
            // Системный трей: показать/скрыть окно, выход.
            let window = app.get_webview_window("main").ok_or("окно main не найдено")?;
            TrayIconBuilder::with_id("main-tray")
                .tooltip("CONVERT")
                .on_tray_icon_event(move |tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { .. } = event {
                        let _ = tray.set_visible(false);
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Закрытие окна сворачивает в трей.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                if let Some(tray) = window.app_handle().tray_by_id("main-tray") {
                    let _ = tray.set_visible(true);
                }
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            scan_file,
            enqueue,
            queue_snapshot,
            queue_pause,
            queue_resume,
            queue_cancel,
            queue_cancel_all,
            queue_retry,
            history,
            history_delete,
            history_clear,
            get_settings,
            save_settings,
            engines_status,
            image_metadata,
            image_palette,
            smart_compress_analyze,
            find_duplicates,
            archive_list,
            pdf_metadata,
            pdf_text,
            ocr_image,
            probe_media,
            presets_list,
            preset_save,
            preset_delete,
            workflows_list,
            workflow_save,
            workflow_delete,
            pick_files,
            show_in_folder,
            compare_files,
            assistant_ask,
            enhance_image,
            workflow_run,
            watch_rules,
            watch_upsert,
            watch_remove,
            watch_set_enabled,
            watch_events,
            watch_confirm,
            startup_file,
            image_strip_metadata,
            image_metadata_edit,
        ])
        .run(tauri::generate_context!())
        .expect("ошибка запуска CONVERT");
}

/// Каталог данных приложения ( %APPDATA%/CONVERT на Windows).
fn app_data_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("APPDATA") {
        return std::path::PathBuf::from(dir).join("CONVERT");
    }
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        return std::path::PathBuf::from(dir).join("convert");
    }
    std::path::PathBuf::from(".convert-data")
}

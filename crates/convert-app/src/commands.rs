//! Команды приложения: тонкие функции, вызываемые Tauri-оболочкой.
//! Каждая возвращает ConvertResult<serde_json::Value>, ошибки
//! сериализуются в строки (реализация Serialize у ConvertError).

use crate::context::AppContext;
use convert_core::ConvertResult;
use convert_pipeline::{Priority, Task, TaskKind, TaskStatus};
use std::sync::Arc;

/// Smart Scan: информация о файле (формат, размеры, совместимость).
pub fn scan_file(path: &str) -> ConvertResult<serde_json::Value> {
    let info = convert_tools::file_information(std::path::Path::new(path))?;
    let format = convert_core::detect_format_path(std::path::Path::new(path))?;
    let targets = convert_core::conversion_targets(format);
    let mut value = serde_json::to_value(&info).unwrap_or_default();
    // Совместимые форматы с пояснениями.
    let targets_json: Vec<serde_json::Value> = targets
        .iter()
        .map(|t| {
            serde_json::json!({
                "format": t.target.display_name(),
                "extension": t.target.extension(),
                "note": t.note,
            })
        })
        .collect();
    value["targets"] = serde_json::Value::Array(targets_json);
    Ok(value)
}

/// Добавить задачу в очередь; возвращает id.
pub fn enqueue(ctx: &Arc<AppContext>, kind: TaskKind, priority: Priority) -> u64 {
    ctx.queue.push(kind, priority)
}

/// Снимок очереди.
pub fn queue_snapshot(ctx: &Arc<AppContext>) -> Vec<Task> {
    ctx.queue.snapshot()
}

/// Пауза/продолжение/отмена.
pub fn queue_pause(ctx: &Arc<AppContext>) {
    ctx.queue.pause();
}

pub fn queue_resume(ctx: &Arc<AppContext>) {
    ctx.queue.resume();
}

pub fn queue_cancel(ctx: &Arc<AppContext>, id: u64) -> bool {
    ctx.queue.cancel(id)
}

pub fn queue_cancel_all(ctx: &Arc<AppContext>) -> usize {
    ctx.queue.cancel_all()
}

/// Повтор неудачной задачи.
pub fn queue_retry(ctx: &Arc<AppContext>, id: u64) -> Option<u64> {
    ctx.queue.retry(id)
}

/// История операций.
pub fn history(
    ctx: &Arc<AppContext>,
    query: Option<String>,
    status: Option<String>,
    limit: u32,
) -> ConvertResult<Vec<convert_db::HistoryEntry>> {
    let db = ctx.db.lock().unwrap();
    db.history(query.as_deref(), status.as_deref(), limit)
}

/// Удалить запись истории.
pub fn history_delete(ctx: &Arc<AppContext>, id: i64) -> ConvertResult<()> {
    let db = ctx.db.lock().unwrap();
    db.delete_history(id)
}

/// Очистить историю.
pub fn history_clear(ctx: &Arc<AppContext>) -> ConvertResult<()> {
    let db = ctx.db.lock().unwrap();
    db.clear_history()
}

/// Настройки: чтение.
pub fn get_settings(ctx: &Arc<AppContext>) -> convert_core::AppSettings {
    ctx.settings()
}

/// Настройки: сохранение.
pub fn save_settings(ctx: &Arc<AppContext>, settings: convert_core::AppSettings) -> ConvertResult<()> {
    ctx.set_settings(&settings)
}

/// Статус внешних движков (страница «Установленные компоненты»).
pub fn engines_status() -> Vec<convert_media::EngineStatus> {
    convert_media::engines_status()
}

/// Пресеты.
pub fn presets_list(ctx: &Arc<AppContext>, tool: &str) -> ConvertResult<Vec<(String, String)>> {
    let db = ctx.db.lock().unwrap();
    db.presets(tool)
}

pub fn preset_save(ctx: &Arc<AppContext>, name: &str, tool: &str, settings_json: &str) -> ConvertResult<i64> {
    let db = ctx.db.lock().unwrap();
    db.save_preset(name, tool, settings_json)
}

pub fn preset_delete(ctx: &Arc<AppContext>, name: &str) -> ConvertResult<()> {
    let db = ctx.db.lock().unwrap();
    db.delete_preset(name)
}

/// Сценарии Workflow.
pub fn workflows_list(ctx: &Arc<AppContext>) -> ConvertResult<Vec<(String, String)>> {
    let db = ctx.db.lock().unwrap();
    db.workflows()
}

pub fn workflow_save(ctx: &Arc<AppContext>, name: &str, steps_json: &str) -> ConvertResult<i64> {
    let db = ctx.db.lock().unwrap();
    db.save_workflow(name, steps_json)
}

pub fn workflow_delete(ctx: &Arc<AppContext>, name: &str) -> ConvertResult<()> {
    let db = ctx.db.lock().unwrap();
    db.delete_workflow(name)
}

/// Метаданные изображения.
pub fn image_metadata(path: &str) -> ConvertResult<convert_image::ImageMetadata> {
    convert_image::read_image_metadata(std::path::Path::new(path))
}

/// Палитра изображения.
pub fn image_palette(path: &str, max_colors: usize) -> ConvertResult<Vec<convert_image::PaletteColor>> {
    let img = convert_image::load_image(std::path::Path::new(path))?;
    Ok(convert_image::extract_palette(&img, max_colors))
}

/// Анализ Smart Compress.
pub fn smart_compress_analyze(
    path: &str,
    goal: convert_tools::CompressGoal,
) -> ConvertResult<convert_tools::SmartCompressPlan> {
    convert_tools::analyze_image(
        std::path::Path::new(path),
        convert_image::ImageOutputFormat::Jpeg,
        goal,
    )
}

/// Дубликаты в папке.
pub fn find_duplicates(root: &str) -> ConvertResult<Vec<convert_tools::DuplicateGroup>> {
    convert_tools::find_duplicates(std::path::Path::new(root))
}

/// Содержимое архива.
pub fn archive_list(path: &str) -> ConvertResult<Vec<convert_archive::ArchiveEntry>> {
    convert_archive::list_archive(std::path::Path::new(path))
}

/// PDF: метаданные.
pub fn pdf_metadata(path: &str) -> ConvertResult<convert_pdf::PdfMetadata> {
    convert_pdf::read_pdf_metadata(std::path::Path::new(path))
}

/// PDF: текст.
pub fn pdf_text(path: &str) -> ConvertResult<String> {
    convert_pdf::extract_text(std::path::Path::new(path))
}

/// OCR изображения → текст.
pub fn ocr_image(path: &str, langs: &str) -> ConvertResult<convert_media::OcrOutput> {
    convert_media::ocr_image(std::path::Path::new(path), langs)
}

/// Медиа-инфо.
pub fn probe_media(path: &str) -> ConvertResult<convert_media::MediaInfo> {
    convert_media::probe_media(std::path::Path::new(path))
}

// ---- File Compare (ТЗ §7) ---------------------------------------------------

/// Сравнение двух файлов (изображения с превью, тексты с diff, медиа).
pub fn compare_files(left: &str, right: &str) -> ConvertResult<convert_tools::CompareReport> {
    convert_tools::compare_files(std::path::Path::new(left), std::path::Path::new(right))
}

// ---- AI File Assistant (ТЗ §5.2, локальный rule-based) ------------------------

/// Запрос к ассистенту: план + предлагаемые задачи (нужны подтверждения).
pub fn assistant_ask(query: &str, files: &[String]) -> convert_tools::AssistantReply {
    convert_tools::ask(query, files)
}

// ---- Quality Enhancer (ТЗ §4.3) ------------------------------------------------

/// Улучшение изображения с уровнями (light/medium/strong).
pub fn enhance_image(
    path: &str,
    output: &str,
    level: &str,
) -> ConvertResult<convert_tools::EnhanceResult> {
    let level = match level {
        "light" => convert_tools::EnhanceLevel::Light,
        "strong" => convert_tools::EnhanceLevel::Strong,
        _ => convert_tools::EnhanceLevel::Medium,
    };
    convert_tools::enhance(std::path::Path::new(path), std::path::Path::new(output), level)
}

// ---- Workflow Builder (ТЗ §5.3) -------------------------------------------------

/// Запуск сценария над файлами (синхронно — UI вызывает в очереди).
pub fn workflow_run(
    ctx: &Arc<AppContext>,
    name: &str,
    inputs: &[String],
) -> ConvertResult<crate::workflows::WorkflowRunReport> {
    // Сценарий из БД.
    let steps_json = {
        let db = ctx.db.lock().unwrap();
        db.workflows()?
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, json)| json)
            .ok_or_else(|| {
                convert_core::ConvertError::InvalidParams(format!("сценарий «{name}» не найден"))
            })?
    };
    let workflow: crate::workflows::Workflow = serde_json::from_str(&steps_json)
        .map_err(|e| convert_core::ConvertError::Processing(format!("сценарий повреждён: {e}")))?;
    let files: Vec<std::path::PathBuf> = inputs.iter().map(std::path::PathBuf::from).collect();
    crate::workflows::run_workflow(&workflow, &files)
}

// ---- Автосценарии (ТЗ §5.5) -------------------------------------------------------

/// Список watch-правил.
pub fn watch_rules(watcher: &std::sync::Arc<crate::watch::WatchService>) -> Vec<crate::watch::WatchRule> {
    watcher.rules()
}

/// Добавить/обновить watch-правило.
pub fn watch_upsert(watcher: &std::sync::Arc<crate::watch::WatchService>, rule: crate::watch::WatchRule) {
    watcher.upsert_rule(rule);
}

/// Удалить watch-правило.
pub fn watch_remove(watcher: &std::sync::Arc<crate::watch::WatchService>, id: u64) {
    watcher.remove_rule(id);
}

/// Включить/выключить правило.
pub fn watch_set_enabled(watcher: &std::sync::Arc<crate::watch::WatchService>, id: u64, enabled: bool) {
    watcher.set_enabled(id, enabled);
}

/// Забрать накопленные события (предложения для пользователя).
pub fn watch_events(watcher: &std::sync::Arc<crate::watch::WatchService>) -> Vec<crate::watch::WatchEvent> {
    watcher.drain_events()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::build_runner;

    fn test_ctx() -> Arc<AppContext> {
        let dir = std::env::temp_dir().join(format!("convert_app_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ctx = AppContext::new(dir, build_runner_noop(), 2).unwrap();
        Arc::new(ctx)
    }

    fn build_runner_noop() -> convert_pipeline::TaskRunner {
        Arc::new(|_t, _p| {
            Ok(convert_pipeline::TaskOutput { output_path: None })
        })
    }

    #[test]
    fn scan_file_reports_targets() {
        let dir = std::env::temp_dir().join("convert_app_scan");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("img.png");
        image::save_buffer_with_format(
            &png,
            image::RgbaImage::new(10, 10).as_raw(),
            10,
            10,
            image::ExtendedColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .unwrap();

        let info = scan_file(png.to_str().unwrap()).unwrap();
        assert_eq!(info["detected_format"], "PNG");
        assert!(info["targets"].as_array().unwrap().len() >= 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn queue_flow_via_commands() {
        let ctx = test_ctx();
        let id = enqueue(
            &ctx,
            TaskKind::Generic { operation: "test".into(), payload_json: "{}".into() },
            Priority::Normal,
        );
        assert!(ctx.queue.wait_all(std::time::Duration::from_secs(5)));
        let snap = queue_snapshot(&ctx);
        assert!(snap.iter().any(|t| t.id == id && t.status == TaskStatus::Done));

        // История записана автоматически через runner? — наш noop-runner
        // не пишет историю (она пишется в build_runner). Проверим руками.
        // (для noop-runner записи не будет — это нормально)
        let _ = history(&ctx, None, None, 10).unwrap();
    }

    #[test]
    fn settings_roundtrip_via_commands() {
        let ctx = test_ctx();
        let mut s = get_settings(&ctx);
        assert_eq!(s.appearance.theme, "system");
        s.appearance.theme = "dark".into();
        save_settings(&ctx, s).unwrap();
        assert_eq!(get_settings(&ctx).appearance.theme, "dark");
    }

    #[test]
    fn presets_via_commands() {
        let ctx = test_ctx();
        preset_save(&ctx, "my", "image_studio", r#"{"q":80}"#).unwrap();
        let list = presets_list(&ctx, "image_studio").unwrap();
        assert_eq!(list.len(), 1);
        preset_delete(&ctx, "my").unwrap();
        assert!(presets_list(&ctx, "image_studio").unwrap().is_empty());
    }

    #[test]
    fn archive_list_command() {
        let dir = std::env::temp_dir().join("convert_app_arch");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("f.txt"), b"data").unwrap();
        let zip_path = dir.join("a.zip");
        convert_archive::create_archive(
            &[dir.join("f.txt")],
            &zip_path,
            convert_archive::ArchiveFormat::Zip,
            None,
            5,
        )
        .unwrap();
        let entries = archive_list(zip_path.to_str().unwrap()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "f.txt");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runner_routes_pdf_merge() {
        // Проверяем, что реальный runner выполняет Generic pdf_merge.
        let dir = std::env::temp_dir().join("convert_app_merge");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Два одностраничных PDF.
        let make = |name: &str| {
            let p = dir.join(name);
            convert_pdf::images_to_pdf(
                &[{
                    let img = dir.join(format!("{name}.png"));
                    image::save_buffer_with_format(
                        &img,
                        image::RgbaImage::from_fn(20, 20, |_, _| image::Rgba([200, 10, 10, 255])).as_raw(),
                        20,
                        20,
                        image::ExtendedColorType::Rgba8,
                        image::ImageFormat::Png,
                    )
                    .unwrap();
                    img
                }],
                &p,
            )
            .unwrap();
            p
        };
        let a = make("a.pdf");
        let b = make("b.pdf");

        let ctx_dir = std::env::temp_dir().join(format!("convert_app_ctx_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ctx_dir);
        let ctx = Arc::new(AppContext::new(ctx_dir, Arc::new(|_t, _p| {
            panic!("runner не должен вызываться напрямую")
        }), 1).unwrap());
        let runner = build_runner(ctx.clone());

        let merged = dir.join("merged.pdf");
        let payload = serde_json::json!({
            "files": [a.to_str().unwrap(), b.to_str().unwrap()],
            "output": merged.to_str().unwrap(),
        });
        let task = Task {
            id: 1,
            kind: TaskKind::Generic {
                operation: "pdf_merge".into(),
                payload_json: payload.to_string(),
            },
            priority: Priority::Normal,
            status: TaskStatus::Pending,
            error: None,
            output: None,
        };
        let progress: convert_pipeline::ProgressCallback = Arc::new(|_| {});
        let out = runner(&task, &progress).unwrap();
        assert_eq!(out.output_path, Some(merged.clone()));
        assert_eq!(convert_pdf::pdf_page_count(&merged).unwrap(), 2);

        // История записана настоящим runner-ом.
        let entries = history(&ctx, None, None, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, "success");
        assert!(entries[0].source.contains("payload") || entries[0].source.contains("неизвестно") || !entries[0].source.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn runner_executes_workflow_from_db() {
        let dir = std::env::temp_dir().join("convert_app_wf");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Изображение.
        let img_path = dir.join("a.png");
        image::save_buffer_with_format(
            &img_path,
            image::RgbaImage::from_fn(30, 30, |x, _| image::Rgba([(x * 8) as u8, 90, 120, 255])).as_raw(),
            30,
            30,
            image::ExtendedColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .unwrap();

        // Контекст + сценарий в БД.
        let ctx_dir = std::env::temp_dir().join(format!("convert_app_wfctx_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ctx_dir);
        let ctx = Arc::new(AppContext::new(ctx_dir, Arc::new(|_t, _p| unreachable!()), 1).unwrap());
        let workflow_json = serde_json::json!({
            "name": "сжать и в png",
            "steps": [
                { "op": "scan", "only_formats": ["png"] },
                { "op": "compress", "quality": 70 }
            ]
        })
        .to_string();
        ctx.db.lock().unwrap().save_workflow("сжать и в png", &workflow_json).unwrap();

        let runner = build_runner(ctx.clone());
        let task = Task {
            id: 1,
            kind: TaskKind::Generic {
                operation: "workflow_run".into(),
                payload_json: serde_json::json!({
                    "workflow_name": "сжать и в png",
                    "inputs": [img_path.to_str().unwrap()]
                })
                .to_string(),
            },
            priority: Priority::Normal,
            status: TaskStatus::Pending,
            error: None,
            output: None,
        };
        let progress: convert_pipeline::ProgressCallback = Arc::new(|_| {});
        let out = runner(&task, &progress).unwrap();
        let output = out.output_path.expect("workflow обязан дать результат");
        assert!(output.exists(), "нет файла результата: {output:?}");
        assert!(output.to_string_lossy().contains("compressed"));

        // История записана.
        let entries = history(&ctx, None, None, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, "success");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compare_and_assistant_commands() {
        let dir = std::env::temp_dir().join("convert_app_cmp");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(dir.join("b.txt"), "one\nTWO\n").unwrap();

        // Compare.
        let report = compare_files(
            dir.join("a.txt").to_str().unwrap(),
            dir.join("b.txt").to_str().unwrap(),
        )
        .unwrap();
        assert!(!report.identical);
        assert!(report.kind == convert_tools::CompareKind::Text);

        // Assistant.
        let reply = assistant_ask("найди одинаковые файлы", &[]);
        assert_eq!(reply.intent, "поиск дубликатов");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn enhance_command_produces_file() {
        let dir = std::env::temp_dir().join("convert_app_enh");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("in.png");
        image::save_buffer_with_format(
            &src,
            image::RgbImage::from_fn(50, 40, |x, y| image::Rgb([(x * 5) as u8, (y * 6) as u8, 77])).as_raw(),
            50,
            40,
            image::ExtendedColorType::Rgb8,
            image::ImageFormat::Png,
        )
        .unwrap();
        let out = dir.join("out.png");
        let result = enhance_image(src.to_str().unwrap(), out.to_str().unwrap(), "medium").unwrap();
        assert!(out.exists());
        assert!(result.disclaimer.contains("не восстанавливает"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! Исполнитель задач очереди: маршрутизация TaskKind по движкам.
//!
//! Здесь замыкание, которое BatchQueue вызывает для каждой задачи:
//! распаковываем JSON параметров, вызываем соответствующий движок,
//! пишем историю в БД.

use crate::context::AppContext;
use convert_core::ConvertResult;
use convert_pipeline::{Task, TaskKind, TaskOutput, TaskRunner};
use std::sync::Arc;

/// Собирает исполнитель задач для данного контекста.
pub fn build_runner(ctx: Arc<AppContext>) -> TaskRunner {
    Arc::new(move |task: &Task, _progress| execute_task(&ctx, task))
}

fn execute_task(ctx: &Arc<AppContext>, task: &Task) -> ConvertResult<TaskOutput> {
    let started = convert_pipeline::now_ts();

    // Workflow-задачи исполняются отдельно: нужен доступ к БД со сценариями.
    let result = match &task.kind {
        TaskKind::Generic { operation, payload_json } if operation == "workflow_run" => {
            run_workflow_task(ctx, payload_json).map(|path| TaskOutput { output_path: path })
        }
        _ => run_kind(&task.kind),
    };
    // История пишется для каждой завершённой задачи.
    let mut finished_task = task.clone();
    match &result {
        Ok(out) => {
            finished_task.status = convert_pipeline::TaskStatus::Done;
            finished_task.output = out.output_path.clone();
        }
        Err(e) => {
            finished_task.status = convert_pipeline::TaskStatus::Error;
            finished_task.error = Some(e.to_string());
        }
    }
    if let Ok(db) = ctx.db.lock() {
        let _ = convert_pipeline::record_history(&db, &finished_task, started);
    }
    result
}

fn run_kind(kind: &TaskKind) -> ConvertResult<TaskOutput> {    match kind {
        TaskKind::ImageJob { payload_json } => {
            let job: convert_image::ImageJob = serde_json::from_str(payload_json)
                .map_err(|e| convert_core::ConvertError::InvalidParams(format!("параметры: {e}")))?;
            let result = convert_image::run_job(&job)?;
            Ok(TaskOutput { output_path: Some(result.output_path) })
        }
        TaskKind::VideoJob { payload_json } => {
            let job: convert_media::VideoJob = serde_json::from_str(payload_json)
                .map_err(|e| convert_core::ConvertError::InvalidParams(format!("параметры: {e}")))?;
            let out = convert_media::run_video_job(&job)?;
            Ok(TaskOutput { output_path: Some(out) })
        }
        TaskKind::AudioJob { payload_json } => {
            let job: convert_media::AudioJob = serde_json::from_str(payload_json)
                .map_err(|e| convert_core::ConvertError::InvalidParams(format!("параметры: {e}")))?;
            let out = convert_media::run_audio_job(&job)?;
            Ok(TaskOutput { output_path: Some(out) })
        }
        TaskKind::OfficeConvert { source, target, output_dir } => {
            let fmt = parse_office_format(target)?;
            let out = convert_media::office_convert(source, fmt, output_dir.as_deref())?;
            Ok(TaskOutput { output_path: Some(out) })
        }
        TaskKind::Generic { operation, payload_json } => {
            run_generic(operation, payload_json)
        }
    }
}

fn parse_office_format(target: &str) -> ConvertResult<convert_media::LibreOfficeFormat> {
    use convert_media::LibreOfficeFormat as F;
    Ok(match target.to_lowercase().as_str() {
        "pdf" => F::Pdf,
        "docx" => F::Docx,
        "odt" => F::Odt,
        "rtf" => F::Rtf,
        "txt" => F::Txt,
        "xlsx" => F::Xlsx,
        "ods" => F::Ods,
        "csv" => F::Csv,
        "pptx" => F::Pptx,
        "odp" => F::Odp,
        "html" => F::Html,
        other => {
            return Err(convert_core::ConvertError::InvalidParams(format!(
                "неизвестный формат документа: {other}"
            )))
        }
    })
}

/// Произвольные операции через Generic-задачи.
fn run_generic(operation: &str, payload_json: &str) -> ConvertResult<TaskOutput> {
    let value: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|e| convert_core::ConvertError::InvalidParams(format!("параметры: {e}")))?;
    let get_path = |key: &str| -> ConvertResult<std::path::PathBuf> {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from)
            .ok_or_else(|| {
                convert_core::ConvertError::InvalidParams(format!("нет параметра {key}"))
            })
    };

    match operation {
        "pdf_merge" => {
            let files: Vec<std::path::PathBuf> = value
                .get("files")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|f| f.as_str())
                        .map(std::path::PathBuf::from)
                        .collect()
                })
                .ok_or_else(|| {
                    convert_core::ConvertError::InvalidParams("нет параметра files".into())
                })?;
            let output = get_path("output")?;
            convert_pdf::merge_pdfs(&files, &output)?;
            Ok(TaskOutput { output_path: Some(output) })
        }
        "pdf_split" => {
            let source = get_path("source")?;
            let output_dir = get_path("output_dir")?;
            let files = convert_pdf::split_pdf(&source, &output_dir)?;
            Ok(TaskOutput { output_path: files.first().cloned() })
        }
        "pdf_watermark" => {
            let source = get_path("source")?;
            let output = get_path("output")?;
            let text = value.get("text").and_then(|v| v.as_str()).unwrap_or("CONVERT");
            convert_pdf::add_text_watermark(
                &source,
                &output,
                &convert_pdf::WatermarkOptions {
                    text: text.to_string(),
                    ..Default::default()
                },
            )?;
            Ok(TaskOutput { output_path: Some(output) })
        }
        "pdf_numbers" => {
            let source = get_path("source")?;
            let output = get_path("output")?;
            convert_pdf::add_page_numbers(&source, &output, convert_pdf::NumberPosition::BottomCenter)?;
            Ok(TaskOutput { output_path: Some(output) })
        }
        "archive_create" => {
            let items: Vec<std::path::PathBuf> = value
                .get("items")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|f| f.as_str())
                        .map(std::path::PathBuf::from)
                        .collect()
                })
                .ok_or_else(|| {
                    convert_core::ConvertError::InvalidParams("нет параметра items".into())
                })?;
            let output = get_path("output")?;
            let format = match value.get("format").and_then(|v| v.as_str()) {
                Some("zip") => convert_archive::ArchiveFormat::Zip,
                Some("7z") => convert_archive::ArchiveFormat::SevenZip,
                Some("tar") => convert_archive::ArchiveFormat::Tar,
                Some("tar.gz") | Some("targz") => convert_archive::ArchiveFormat::TarGz,
                _ => convert_archive::ArchiveFormat::Zip,
            };
            let password = value.get("password").and_then(|v| v.as_str());
            convert_archive::create_archive(&items, &output, format, password, 6)?;
            Ok(TaskOutput { output_path: Some(output) })
        }
        "archive_extract" => {
            let archive = get_path("archive")?;
            let output_dir = get_path("output_dir")?;
            let password = value.get("password").and_then(|v| v.as_str());
            let limits = convert_archive::ExtractLimits::default();
            let files = convert_archive::extract_archive(&archive, &output_dir, password, &limits)?;
            Ok(TaskOutput { output_path: files.first().cloned() })
        }
        "ocr" => {
            let image = get_path("source")?;
            let langs = value.get("langs").and_then(|v| v.as_str()).unwrap_or("eng");
            let output = value.get("output").and_then(|v| v.as_str());
            match output {
                Some(out) if out.ends_with(".docx") => {
                    let result = convert_media::ocr_to_docx(&image, langs, std::path::Path::new(out))?;
                    Ok(TaskOutput { output_path: result.output_path })
                }
                _ => {
                    let result = convert_media::ocr_image(&image, langs)?;
                    Ok(TaskOutput { output_path: None })
                        .map(|mut r| {
                            r.output_path = None;
                            r
                        })
                        .map(|r| {
                            // Текст возвращается в payload результата
                            // через сохранение в txt рядом.
                            r
                        })
                }
            }
        }
        "qr" => {
            let text = value
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    convert_core::ConvertError::InvalidParams("нет параметра text".into())
                })?;
            let output = get_path("output")?;
            convert_tools::generate_qr_png(text, &convert_tools::QrStyle::default(), &output)?;
            Ok(TaskOutput { output_path: Some(output) })
        }
        "enhance" => {
            // Quality Enhancer: уровень + честный дисклеймер в результате.
            let source = get_path("source")?;
            let level = match value.get("level").and_then(|v| v.as_str()) {
                Some("light") => convert_tools::EnhanceLevel::Light,
                Some("strong") => convert_tools::EnhanceLevel::Strong,
                _ => convert_tools::EnhanceLevel::Medium,
            };
            let parent = source.parent().unwrap_or(std::path::Path::new("."));
            let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("img");
            let out = parent.join(format!("{stem}_enhanced.png"));
            let result = convert_tools::enhance(&source, &out, level)?;
            Ok(TaskOutput { output_path: Some(std::path::PathBuf::from(result.output_path)) })
        }
        "encoding_convert" => {
            let source = get_path("source")?;
            let output = get_path("output")?;
            let to = match value.get("to").and_then(|v| v.as_str()) {
                Some("utf-8") => convert_tools::EncodingName::Utf8,
                Some("windows-1251") => convert_tools::EncodingName::Windows1251,
                Some("koi8-r") => convert_tools::EncodingName::Koi8r,
                Some("utf-16le") => convert_tools::EncodingName::Utf16Le,
                Some("utf-16be") => convert_tools::EncodingName::Utf16Be,
                Some("latin-1") => convert_tools::EncodingName::Latin1,
                _ => convert_tools::EncodingName::Utf8,
            };
            convert_tools::convert_encoding(&source, output.clone(), None, to)?;
            Ok(TaskOutput { output_path: Some(output) })
        }
        "smart_compress" => {
            let source = get_path("source")?;
            let output = get_path("output")?;
            let quality = value.get("quality").and_then(|v| v.as_u64()).unwrap_or(75) as u8;
            let out = convert_tools::smart_compress_image(
                &source,
                &output,
                convert_image::ImageOutputFormat::Jpeg,
                quality,
            )?;
            let _ = out;
            Ok(TaskOutput { output_path: Some(output) })
        }
        "workflow_run" => {
            // Обрабатывается на уровне execute_task (нужен доступ к БД
            // для загрузки сценария); сюда попадание = внутренняя ошибка.
            Err(convert_core::ConvertError::Processing(
                "workflow_run: внутренняя ошибка маршрутизации".into(),
            ))
        }
        other => Err(convert_core::ConvertError::InvalidParams(format!(
            "неизвестная операция: {other}"
        ))),
    }
}

/// Исполнение workflow-задачи: загрузка сценария из БД и запуск.
fn run_workflow_task(ctx: &Arc<AppContext>, payload_json: &str) -> ConvertResult<Option<std::path::PathBuf>> {
    let value: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|e| convert_core::ConvertError::InvalidParams(format!("параметры: {e}")))?;
    let name = value
        .get("workflow_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            convert_core::ConvertError::InvalidParams("нет параметра workflow_name".into())
        })?;
    let inputs: Vec<std::path::PathBuf> = value
        .get("inputs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| f.as_str())
                .map(std::path::PathBuf::from)
                .collect()
        })
        .unwrap_or_default();

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

    let report = crate::workflows::run_workflow(&workflow, &inputs)?;
    Ok(report.outputs.first().map(std::path::PathBuf::from))
}

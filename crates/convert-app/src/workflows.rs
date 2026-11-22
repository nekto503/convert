//! Workflow Builder — исполнение сценариев (ТЗ §5.3).
//!
//! Сценарий — цепочка шагов; выход каждого шага — вход следующего.
//! Шаги:
//! - `scan`      — Smart Scan (фильтрация по формату);
//! - `compress`  — сжатие (Smart Compress с уровнем);
//! - `convert`   — конвертация (формат из payload);
//! - `resize`    — изменение размера изображений;
//! - `archive`   — упаковка результатов в ZIP;
//! - `ocr`       — распознавание текста.
//!
//! Условия (ТЗ): шаг `scan` с параметром `only_formats` пропускает
//! дальше только файлы заданных форматов — это условный шаг.
//! Сценарии сохраняются в SQLite (convert-db: workflows) и запускаются
//! повторно. Деструктивных шагов нет: всё пишет в новые файлы.

use convert_core::error::{ConvertError, ConvertResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Один шаг сценария.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WorkflowStep {
    /// Фильтр: пропустить только указанные форматы (условие).
    Scan { only_formats: Vec<String> },
    /// Сжатие изображений с качеством.
    Compress { quality: u8 },
    /// Конвертация изображений в формат (png/jpeg/webp).
    ConvertImage { format: String, quality: u8 },
    /// Масштабирование изображений по ширине.
    Resize { width: u32 },
    /// Распознавание текста (OCR) — текст пишется в .txt рядом.
    Ocr { langs: String },
    /// Упаковать все результаты в ZIP.
    Archive { name: String },
}

/// Сценарий целиком.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    pub steps: Vec<WorkflowStep>,
}

/// Отчёт о выполнении сценария.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunReport {
    /// Входные файлы.
    pub inputs: Vec<String>,
    /// Файлы, прошедшие все шаги (после условий).
    pub processed: Vec<String>,
    /// Результаты последнего шага.
    pub outputs: Vec<String>,
    /// Что сделал каждый шаг.
    pub step_logs: Vec<String>,
    pub success: bool,
}

/// Выполняет сценарий над файлами.
///
/// Файлы проходят цепочку по одному (scan/compress/convert/resize/ocr);
/// archive применяется к накопленным результатам.
pub fn run_workflow(workflow: &Workflow, inputs: &[PathBuf]) -> ConvertResult<WorkflowRunReport> {
    if inputs.is_empty() {
        return Err(ConvertError::InvalidParams("нет входных файлов".into()));
    }
    let mut current: Vec<PathBuf> = inputs.to_vec();
    let mut logs = Vec::new();
    let mut outputs: Vec<PathBuf> = Vec::new();

    for (i, step) in workflow.steps.iter().enumerate() {
        match step {
            WorkflowStep::Scan { only_formats } => {
                let before = current.len();
                current = current
                    .into_iter()
                    .filter(|p| {
                        let ext = p
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.to_lowercase())
                            .unwrap_or_default();
                        let fmt = convert_core::detect_format_path(p)
                            .map(|f| f.extension().to_string())
                            .unwrap_or_default();
                        only_formats
                            .iter()
                            .any(|allowed| allowed.eq_ignore_ascii_case(&ext) || allowed.eq_ignore_ascii_case(&fmt))
                    })
                    .collect();
                logs.push(format!(
                    "Шаг {} (scan): пропущено {}/{} файлов по фильтру {:?}",
                    i + 1,
                    current.len(),
                    before,
                    only_formats
                ));
                if current.is_empty() {
                    return Ok(WorkflowRunReport {
                        inputs: inputs.iter().map(|p| p.display().to_string()).collect(),
                        processed: vec![],
                        outputs: vec![],
                        step_logs: logs,
                        success: true,
                    });
                }
            }
            WorkflowStep::Compress { quality } => {
                let mut next = Vec::new();
                for p in &current {
                    let out = suffixed(p, "_compressed", "jpg");
                    convert_tools::smart_compress_image(p, &out, convert_image::ImageOutputFormat::Jpeg, *quality)?;
                    next.push(out);
                }
                logs.push(format!("Шаг {} (compress): сжато {} файлов до качества {quality}", i + 1, next.len()));
                current = next;
            }
            WorkflowStep::ConvertImage { format, quality } => {
                let target = match format.to_lowercase().as_str() {
                    "png" => convert_image::ImageOutputFormat::Png,
                    "jpg" | "jpeg" => convert_image::ImageOutputFormat::Jpeg,
                    "webp" => convert_image::ImageOutputFormat::WebP { lossless: false },
                    "bmp" => convert_image::ImageOutputFormat::Bmp,
                    "tiff" => convert_image::ImageOutputFormat::Tiff,
                    other => {
                        return Err(ConvertError::InvalidParams(format!(
                            "workflow: формат {other} не поддерживается шагом convert"
                        )))
                    }
                };
                let mut next = Vec::new();
                for p in &current {
                    let img = convert_image::load_image(p)?;
                    let out = with_ext(p, &format!("converted_{}", target.extension()));
                    convert_image::encode::encode_to_file(
                        &img,
                        target,
                        &convert_image::EncodeParams { quality: *quality, dpi: None },
                        &out,
                    )?;
                    next.push(out);
                }
                logs.push(format!("Шаг {} (convert): {} файлов → {format}", i + 1, next.len()));
                current = next;
            }
            WorkflowStep::Resize { width } => {
                let mut next = Vec::new();
                for p in &current {
                    let img = convert_image::load_image(p)?;
                    let resized = convert_image::ops::resize(
                        &img,
                        &convert_image::ops::ResizeParams {
                            width: *width,
                            height: *width,
                            mode: convert_image::ops::ResizeMode::Fit,
                            high_quality: true,
                        },
                    )
                    .map_err(ConvertError::InvalidParams)?;
                    let out = suffixed(p, "_resized", "png");
                    convert_image::encode::encode_to_file(
                        &resized,
                        convert_image::ImageOutputFormat::Png,
                        &Default::default(),
                        &out,
                    )?;
                    next.push(out);
                }
                logs.push(format!("Шаг {} (resize): {} файлов до ширины {width}px", i + 1, next.len()));
                current = next;
            }
            WorkflowStep::Ocr { langs } => {
                let mut next = Vec::new();
                for p in &current {
                    let result = convert_media::ocr_image(p, langs)?;
                    let out = with_ext(p, "ocr.txt");
                    std::fs::write(&out, &result.text)?;
                    next.push(out);
                }
                logs.push(format!("Шаг {} (ocr): распознано {} файлов ({langs})", i + 1, next.len()));
                current = next;
            }
            WorkflowStep::Archive { name } => {
                let base = current
                    .first()
                    .and_then(|p| p.parent())
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."));
                let out = base.join(format!("{name}.zip"));
                convert_archive::create_archive(&current, &out, convert_archive::ArchiveFormat::Zip, None, 6)?;
                logs.push(format!("Шаг {} (archive): {} файлов → {}", i + 1, current.len(), out.display()));
                outputs = vec![out];
            }
        }
    }
    // Если archive не было — результат = текущие файлы.
    if outputs.is_empty() {
        outputs = current.clone();
    }

    Ok(WorkflowRunReport {
        inputs: inputs.iter().map(|p| p.display().to_string()).collect(),
        processed: current.iter().map(|p| p.display().to_string()).collect(),
        outputs: outputs.iter().map(|p| p.display().to_string()).collect(),
        step_logs: logs,
        success: true,
    })
}

fn with_ext(path: &Path, stem_suffix_and_ext: &str) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    match path.parent() {
        Some(parent) => parent.join(format!("{stem}{stem_suffix_and_ext}")),
        None => PathBuf::from(format!("{stem}{stem_suffix_and_ext}")),
    }
}

fn suffixed(path: &Path, suffix: &str, ext: &str) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    match path.parent() {
        Some(parent) => parent.join(format!("{stem}{suffix}.{ext}")),
        None => PathBuf::from(format!("{stem}{suffix}.{ext}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_images(dir: &Path, n: usize) -> Vec<PathBuf> {
        (0..n)
            .map(|i| {
                let img = image::RgbImage::from_fn(80, 60, |x, _| image::Rgb([(x * 3 + i as u32 * 40) as u8, 90, 140]));
                let p = dir.join(format!("img_{i}.png"));
                image::save_buffer_with_format(
                    &p,
                    img.as_raw(),
                    80,
                    60,
                    image::ExtendedColorType::Rgb8,
                    image::ImageFormat::Png,
                )
                .unwrap();
                p
            })
            .collect()
    }

    fn make_txt(dir: &Path) -> PathBuf {
        let p = dir.join("note.txt");
        std::fs::write(&p, "текстовый файл").unwrap();
        p
    }

    #[test]
    fn pipeline_compress_then_convert_webp() {
        let dir = std::env::temp_dir().join("convert_wf1");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let images = make_images(&dir, 2);

        // Скан пропускает только png (текстовый отсеется).
        let workflow = Workflow {
            name: "веб-публикация".into(),
            steps: vec![
                WorkflowStep::Scan { only_formats: vec!["png".into()] },
                WorkflowStep::Compress { quality: 75 },
                WorkflowStep::ConvertImage { format: "png".into(), quality: 90 },
            ],
        };
        let mut inputs = images.clone();
        inputs.push(make_txt(&dir));

        let report = run_workflow(&workflow, &inputs).unwrap();
        assert_eq!(report.step_logs.len(), 3);
        // txt отфильтрован.
        assert!(report.processed.len() == 2);
        assert!(report.outputs.len() == 2);
        assert!(report.outputs.iter().all(|o| o.contains("converted_png")));
        for o in &report.outputs {
            assert!(Path::new(o).exists(), "нет файла {o}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pipeline_with_archive() {
        let dir = std::env::temp_dir().join("convert_wf2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let images = make_images(&dir, 2);

        let workflow = Workflow {
            name: "архив".into(),
            steps: vec![
                WorkflowStep::Compress { quality: 70 },
                WorkflowStep::Archive { name: "results".into() },
            ],
        };
        let report = run_workflow(&workflow, &images).unwrap();
        assert_eq!(report.outputs.len(), 1);
        assert!(report.outputs[0].ends_with("results.zip"));
        let entries = convert_archive::list_archive(Path::new(&report.outputs[0])).unwrap();
        assert_eq!(entries.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resize_step_works() {
        let dir = std::env::temp_dir().join("convert_wf3");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let images = make_images(&dir, 1);

        let workflow = Workflow {
            name: "resize".into(),
            steps: vec![WorkflowStep::Resize { width: 40 }],
        };
        let report = run_workflow(&workflow, &images).unwrap();
        let out = image::open(&report.outputs[0]).unwrap();
        assert!(out.width() <= 40, "ширина {} > 40", out.width());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_filters_everything() {
        let dir = std::env::temp_dir().join("convert_wf4");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let txt = make_txt(&dir);
        let workflow = Workflow {
            name: "только фото".into(),
            steps: vec![WorkflowStep::Scan { only_formats: vec!["png".into()] }],
        };
        let report = run_workflow(&workflow, &[txt]).unwrap();
        assert!(report.processed.is_empty());
        assert!(report.success);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_inputs_rejected() {
        let workflow = Workflow { name: "x".into(), steps: vec![] };
        assert!(run_workflow(&workflow, &[]).is_err());
    }

    #[test]
    fn unknown_format_rejected() {
        let dir = std::env::temp_dir().join("convert_wf5");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let images = make_images(&dir, 1);
        let workflow = Workflow {
            name: "bad".into(),
            steps: vec![WorkflowStep::ConvertImage { format: "xyz".into(), quality: 80 }],
        };
        assert!(run_workflow(&workflow, &images).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn workflow_json_roundtrip() {
        // Сценарии хранятся в SQLite как JSON — проверяем сериализацию.
        let wf = Workflow {
            name: "тест".into(),
            steps: vec![
                WorkflowStep::Scan { only_formats: vec!["png".into(), "jpg".into()] },
                WorkflowStep::Compress { quality: 80 },
                WorkflowStep::Archive { name: "out".into() },
            ],
        };
        let json = serde_json::to_string(&wf).unwrap();
        let back: Workflow = serde_json::from_str(&json).unwrap();
        assert_eq!(back.steps.len(), 3);
        assert!(json.contains("compress"));
    }
}

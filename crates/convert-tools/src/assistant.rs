//! AI File Assistant (ТЗ §5.2) — локальный помощник на правилах,
//! без облачного ИИ (что и требует ТЗ).
//!
//! Пользователь пишет задачу обычным языком; помощник:
//! - распознаёт намерение по ключевым словам;
//! - объясняет, какие инструменты подходят;
//! - даёт пошаговую инструкцию;
//! - рекомендует настройки;
//! - формирует готовые задачи для очереди (после подтверждения).

use convert_core::format::FileFormat;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Ответ помощника.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantReply {
    /// Распознанное намерение.
    pub intent: String,
    /// Человекочектибельное описание плана.
    pub explanation: String,
    /// Пошаговая инструкция.
    pub steps: Vec<String>,
    /// Рекомендуемые настройки (JSON для подстановки в UI).
    pub suggested_settings_json: Option<String>,
    /// Готовые задачи для очереди (нужно подтверждение пользователя!).
    pub proposed_tasks: Vec<ProposedTask>,
    /// Предупреждения.
    pub warnings: Vec<String>,
}

/// Задача, предлагаемая помощником (до подтверждения).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedTask {
    pub label: String,
    pub payload_json: String,
}

/// Анализирует запрос пользователя; `files` — текущий выбор файлов.
pub fn ask(query: &str, files: &[String]) -> AssistantReply {
    let q = query.to_lowercase();

    // Выявляем форматы, упомянутые в запросе.
    let target = mentioned_format(&q);

    // --- Намерения ---
    if has_any(&q, &["дубликат", "одинаковые файлы", "duplicate"]) {
        return AssistantReply {
            intent: "поиск дубликатов".into(),
            explanation: "Найду файлы с одинаковым содержимым: группировка по размеру, "
                .to_string() + "затем быстрый частичный хэш, затем полный SHA-256.",
            steps: vec![
                "Открой «Инструменты → Дубликаты» или подтверди задачу ниже".into(),
                "Укажи папку для сканирования (рекомендую папку первого добавленного файла)".into(),
                "После сканирования просмотрю группы и решу, какие копии удалить".into(),
            ],
            suggested_settings_json: None,
            proposed_tasks: if files.is_empty() {
                vec![]
            } else {
                let dir = std::path::Path::new(&files[0])
                    .parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| ".".into());
                vec![ProposedTask {
                    label: "Найти дубликаты".into(),
                    payload_json: serde_json::json!({ "scan_root": dir }).to_string(),
                }]
            },
            warnings: vec!["Удаление файлов не выполняется автоматически — только поиск.".into()],
        };
    }

    if has_any(&q, &["уменьш", "сожм", "сжат", "compress", "меньше вес", "оптимизируй размер"])
        && has_any(&q, &["видео", "video", "mp4", "mkv"])
    {
        return AssistantReply {
            intent: "сжатие видео".into(),
            explanation: "Перекодирую видео через FFmpeg с CRF 26 и preset slow: обычно "
                .to_string() + "экономит 40–60% размера при незаметной потере качества.",
            steps: vec![
                "Video Studio → выбери файлы".into(),
                "Контейнер оставь тем же (MP4/H.264 для совместимости)".into(),
                "CRF: 26 (баланс) или 28 (максимальное сжатие)".into(),
                "Preset: slow — медленнее, но меньше файл".into(),
            ],
            suggested_settings_json: Some(
                r#"{"container":"mp4","video_codec":"h264","crf":26,"preset":"slow"}"#.into(),
            ),
            proposed_tasks: files.iter().map(|f| ProposedTask {
                label: format!("Сжать видео: {}", short_name(f)),
                payload_json: serde_json::json!({
                    "kind": "VideoJob",
                    "source": f,
                    "format": { "container": "mp4", "video_codec": "h264", "audio_codec": "aac" },
                    "crf": 26,
                    "preset": "slow",
                    "audio_bitrate_kbps": 128,
                    "overwrite": "Rename"
                })
                .to_string(),
            }).collect(),
            warnings: vec!["Сжатие видео — с потерями; исходник не изменяется, результат пишется в новый файл.".into()],
        };
    }

    if has_any(&q, &["уменьш", "сожм", "сжат", "compress", "меньше вес", "оптимизируй"])
        && has_any(&q, &["изображен", "фото", "картинк", "image", "photo", "png", "jpg"])
    {
        let fmt = target.unwrap_or(FileFormat::WebP);
        return AssistantReply {
            intent: "сжатие изображений".into(),
            explanation: "Подберу качество автоматически (Smart Compress) или использую "
                .to_string() + &format!("конвертацию в {} с качеством 80.", fmt.display_name()),
            steps: vec![
                "Image Studio → выбери изображения".into(),
                format!("Формат: {} (WebP обычно даёт −30–50% размера)", fmt.display_name()),
                "Качество 80 — безопасный баланс для веба".into(),
                "Или «Инструменты → Smart Compress» для авто-подбора качества".into(),
            ],
            suggested_settings_json: Some(r#"{"format":"WebP","quality":80}"#.into()),
            proposed_tasks: files.iter().map(|f| ProposedTask {
                label: format!("Сжать: {}", short_name(f)),
                payload_json: serde_json::json!({
                    "kind": "ImageJob",
                    "source": f,
                    "target_format": "WebP",
                    "encode": { "quality": 80, "dpi": null },
                    "overwrite": "Rename"
                })
                .to_string(),
            }).collect(),
            warnings: vec!["Lossy WebP выполняется через FFmpeg-движок (если FFmpeg установлен).".into()],
        };
    }

    if has_any(&q, &["webp"]) {
        return AssistantReply {
            intent: "конвертация в WebP".into(),
            explanation: "Сконвертирую изображения в WebP: современный формат, "
                .to_string() + "на 25–35% меньше JPEG при том же качестве.",
            steps: vec![
                "Image Studio → выбери файлы".into(),
                "Формат: WebP, качество 80–85".into(),
            ],
            suggested_settings_json: Some(r#"{"format":"WebP","quality":82}"#.into()),
            proposed_tasks: files.iter().map(|f| ProposedTask {
                label: format!("{} → WebP", short_name(f)),
                payload_json: serde_json::json!({
                    "kind": "ImageJob", "source": f, "target_format": "WebP",
                    "encode": { "quality": 82, "dpi": null }, "overwrite": "Rename"
                })
                .to_string(),
            }).collect(),
            warnings: vec![],
        };
    }

    if has_any(&q, &["звук", "аудио", "audio", "извлеки звук", "mp3", "дорожк"])
        && has_any(&q, &["видео", "video", "mp4", "mkv", "извлек"])
    {
        return AssistantReply {
            intent: "извлечение аудио из видео".into(),
            explanation: "Извлеку звуковую дорожку в MP3 192 кбит/с (без перекодирования видео).".into(),
            steps: vec![
                "Video Studio → «Извлечь аудио» или подтверди задачу ниже".into(),
                "Формат MP3 универсален; для архива без потерь выбери FLAC".into(),
            ],
            suggested_settings_json: Some(r#"{"audio_format":"mp3","bitrate":192}"#.into()),
            proposed_tasks: files.iter().map(|f| ProposedTask {
                label: format!("Аудио из: {}", short_name(f)),
                payload_json: serde_json::json!({
                    "kind": "extract_audio", "source": f, "audio_format": "mp3"
                })
                .to_string(),
            }).collect(),
            warnings: vec![],
        };
    }

    if has_any(&q, &["объедин", "склей", "merge", "слишн"]) && has_any(&q, &["pdf"]) {
        return AssistantReply {
            intent: "объединение PDF".into(),
            explanation: "Склею выбранные PDF в один документ (порядок — как в списке файлов).".into(),
            steps: vec![
                "PDF Studio → операция «Объединить в один PDF»".into(),
                "Порядок файлов = порядок в списке; результат рядом с первым файлом".into(),
            ],
            suggested_settings_json: None,
            proposed_tasks: if files.len() >= 2 {
                let output = with_extension(&files[0], "_merged.pdf");
                vec![ProposedTask {
                    label: "Объединить PDF".into(),
                    payload_json: serde_json::json!({
                        "kind": "pdf_merge", "files": files, "output": output
                    })
                    .to_string(),
                }]
            } else {
                vec![]
            },
            warnings: if files.len() < 2 {
                vec!["Нужно минимум два PDF-файла.".into()]
            } else {
                vec![]
            },
        };
    }

    if has_any(&q, &["распознай", "ocr", "текст со скан", "текст с изображен", "текст из pdf"]) {
        return AssistantReply {
            intent: "распознавание текста (OCR)".into(),
            explanation: "Распознаю текст через Tesseract локально (eng; rus — если установлен).".into(),
            steps: vec![
                "OCR Studio → выбери изображения или сканы PDF".into(),
                "Язык: eng / rus / rus+eng".into(),
                "Экспорт: TXT, DOCX или PDF с текстовым слоем (поиск копируется)".into(),
            ],
            suggested_settings_json: Some(r#"{"langs":"eng"}"#.into()),
            proposed_tasks: files.iter().map(|f| ProposedTask {
                label: format!("OCR: {}", short_name(f)),
                payload_json: serde_json::json!({
                    "kind": "ocr", "source": f, "langs": "eng"
                })
                .to_string(),
            }).collect(),
            warnings: vec!["Точность зависит от качества скана; проверяй результат.".into()],
        };
    }

    if has_any(&q, &["конвертируй", "преобразуй", "переведи", "convert"])
        || target.is_some()
    {
        let fmt = target.unwrap_or(FileFormat::Pdf);
        return AssistantReply {
            intent: format!("конвертация в {}", fmt.display_name()),
            explanation: format!(
                "Определю формат каждого файла (по содержимому, не по расширению) и покажу доступные направления. Для {} подходят:. Выберу нужный инструмент по категории файла.",
                fmt.display_name()
            ),
            steps: vec![
                "Добавь файлы на главную страницу (drag & drop)".into(),
                "Smart Scan покажет фактический формат и совместимые цели".into(),
                format!("Выбери {}: для изображений — Image Studio, документов — Document Studio, видео — Video Studio", fmt.display_name()),
            ],
            suggested_settings_json: None,
            proposed_tasks: vec![],
            warnings: vec![],
        };
    }

    // --- Не распознано ---
    AssistantReply {
        intent: "не распознано".into(),
        explanation: "Не понял задачу. Я умею (примеры): «уменьши размер этого видео», "
            .to_string() + "«преобразуй изображения в WebP», «извлеки звук из видео», «объедини эти PDF», «найди одинаковые файлы», «распознай текст со скана».",
        steps: vec![],
        suggested_settings_json: None,
        proposed_tasks: vec![],
        warnings: vec![],
    }
}

fn has_any(q: &str, words: &[&str]) -> bool {
    words.iter().any(|w| q.contains(w))
}

fn mentioned_format(q: &str) -> Option<FileFormat> {
    for (word, fmt) in [
        ("webp", FileFormat::WebP),
        ("png", FileFormat::Png),
        ("jpg", FileFormat::Jpeg),
        ("jpeg", FileFormat::Jpeg),
        ("pdf", FileFormat::Pdf),
        ("mp3", FileFormat::Mp3),
        ("mp4", FileFormat::Mp4),
        ("mkv", FileFormat::Matroska),
        ("gif", FileFormat::Gif),
        ("flac", FileFormat::Flac),
        ("docx", FileFormat::Docx),
        ("xlsx", FileFormat::Xlsx),
        ("wav", FileFormat::Wav),
        ("opus", FileFormat::Opus),
        ("7z", FileFormat::SevenZip),
        ("zip", FileFormat::Zip),
    ] {
        if q.contains(word) {
            return Some(fmt);
        }
    }
    None
}

fn short_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.into())
}

fn with_extension(path: &str, suffix_and_ext: &str) -> String {
    let p = Path::new(path);
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    match p.parent() {
        Some(parent) => parent.join(format!("{stem}{suffix_and_ext}")).display().to_string(),
        None => format!("{stem}{suffix_and_ext}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_video_compress() {
        let r = ask("уменьши размер этого видео", &["/v/clip.mp4".into()]);
        assert_eq!(r.intent, "сжатие видео");
        assert!(r.explanation.contains("FFmpeg"));
        assert!(!r.steps.is_empty());
        assert_eq!(r.proposed_tasks.len(), 1);
        assert!(r.proposed_tasks[0].payload_json.contains("crf"));
        assert!(!r.warnings.is_empty(), "должно быть предупреждение о потерях");
    }

    #[test]
    fn recognizes_webp_batch() {
        let r = ask(
            "преобразуй все изображения в WEBP",
            &["/i/a.png".into(), "/i/b.png".into()],
        );
        assert!(r.intent.contains("WebP"));
        assert_eq!(r.proposed_tasks.len(), 2);
        assert!(r.proposed_tasks.iter().all(|t| t.payload_json.contains("WebP")));
    }

    #[test]
    fn recognizes_extract_audio() {
        let r = ask("извлеки звук из этого видео", &["/v/movie.mkv".into()]);
        assert_eq!(r.intent, "извлечение аудио из видео");
        assert_eq!(r.proposed_tasks.len(), 1);
    }

    #[test]
    fn recognizes_merge_pdf() {
        let r = ask("объедини эти pdf", &["/a/1.pdf".into(), "/a/2.pdf".into()]);
        assert_eq!(r.intent, "объединение PDF");
        assert_eq!(r.proposed_tasks.len(), 1);
        assert!(r.proposed_tasks[0].payload_json.contains("_merged.pdf"));
        // Без файлов — предупреждение.
        let r2 = ask("объедини pdf", &[]);
        assert!(!r2.warnings.is_empty());
    }

    #[test]
    fn recognizes_duplicates_and_ocr() {
        let r = ask("найди одинаковые файлы", &["/docs/x.txt".into()]);
        assert_eq!(r.intent, "поиск дубликатов");
        let r2 = ask("распознай текст со скана", &["/scan/page1.png".into()]);
        assert_eq!(r2.intent, "распознавание текста (OCR)");
    }

    #[test]
    fn unknown_query_gets_help() {
        let r = ask("что такое погода на марсе", &[]);
        assert_eq!(r.intent, "не распознано");
        assert!(r.explanation.contains("умею"));
        assert!(r.proposed_tasks.is_empty());
    }

    #[test]
    fn image_compress_intent() {
        let r = ask("сожми фото", &["/p/holiday.jpg".into()]);
        assert_eq!(r.intent, "сжатие изображений");
        assert!(r.proposed_tasks[0].payload_json.contains("quality"));
    }
}

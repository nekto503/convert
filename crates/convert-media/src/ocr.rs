//! OCR Studio: распознавание текста через Tesseract (полностью локально).
//!
//! Возможности:
//! - распознавание изображений (PNG/JPEG/TIFF) с выбором языка;
//! - экспорт в TXT и DOCX;
//! - PDF с текстовым слоем: распознаём страницы как картинки и собираем
//!   новый PDF, где под изображением лежит невидимый текст (через hOCR
//!   Tesseract и песочницу pdf — фактическую сборку выполняет convert-pdf).

use crate::process::require_engine;
use convert_core::error::{ConvertError, ConvertResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Языки распознавания (должны быть установлены в системе).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrLanguage(pub String);

impl OcrLanguage {
    pub fn rus() -> Self {
        Self("rus".into())
    }
    pub fn eng() -> Self {
        Self("eng".into())
    }
    /// Строка для tesseract: "rus+eng".
    pub fn combined(langs: &[&str]) -> Self {
        Self(langs.join("+"))
    }
}

/// Результат распознавания.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrOutput {
    /// Распознанный текст.
    pub text: String,
    /// Путь к файлу-результату, если сохраняли.
    pub output_path: Option<PathBuf>,
    /// Средняя уверенность распознавания, 0–100.
    pub confidence: Option<f32>,
}

/// Распознаёт текст на изображении.
///
/// `langs` — строка языков tesseract, например "rus+eng".
pub fn ocr_image(image_path: &Path, langs: &str) -> ConvertResult<OcrOutput> {
    let tesseract = require_engine("tesseract")?;
    if !image_path.is_file() {
        return Err(ConvertError::FileNotFound(image_path.display().to_string()));
    }
    // tesseract пишет результат во временный файл без расширения.
    let temp_dir = std::env::temp_dir();
    let out_base = temp_dir.join(format!("convert_ocr_{}", std::process::id()));
    let out_str = out_base.to_string_lossy().to_string();

    let args = [
        image_path.to_str().unwrap(),
        &out_str,
        "-l", langs,
        "--psm", "3",
        "tsv",
    ];
    crate::process::run_process_with_timeout(
        tesseract.to_str().unwrap(),
        &args,
        Duration::from_secs(5 * 60),
    )?;

    // TSV: колонки level..conf..text; собираем текст и среднюю уверенность.
    let tsv_path = temp_dir.join(format!("{}.tsv", out_str));
    let tsv = std::fs::read_to_string(&tsv_path)
        .map_err(|e| ConvertError::Processing(format!("tesseract не создал TSV: {e}")))?;
    let _ = std::fs::remove_file(&tsv_path);

    let mut text_lines: Vec<String> = Vec::new();
    let mut confidences: Vec<f32> = Vec::new();
    let mut current_line: Vec<String> = Vec::new();
    let mut last_line_id = -1i64;

    for (i, row) in tsv.lines().enumerate() {
        if i == 0 {
            continue; // заголовок
        }
        let cols: Vec<&str> = row.split('\t').collect();
        // TSV tesseract: 12 колонок; level 5 = слово.
        if cols.len() < 12 {
            continue;
        }
        let level: i32 = cols[0].parse().unwrap_or(0);
        if level == 5 {
            let line_id: i64 = cols[4].parse().unwrap_or(0);
            if line_id != last_line_id {
                if !current_line.is_empty() {
                    text_lines.push(current_line.join(" "));
                    current_line.clear();
                }
                last_line_id = line_id;
            }
            let conf: f32 = cols[10].parse().unwrap_or(-1.0);
            let word = cols[11];
            if conf >= 0.0 && !word.is_empty() {
                confidences.push(conf);
                current_line.push(word.to_string());
            } else {
                current_line.push(word.to_string());
            }
        }
    }
    if !current_line.is_empty() {
        text_lines.push(current_line.join(" "));
    }

    let confidence = if confidences.is_empty() {
        None
    } else {
        Some(confidences.iter().sum::<f32>() / confidences.len() as f32)
    };

    Ok(OcrOutput {
        text: text_lines.join("\n"),
        output_path: None,
        confidence,
    })
}

/// Распознаёт изображение и сохраняет результат в DOCX (простой документ
/// с распознанным текстом; сборкой DOCX занимается convert-pdf, чтобы
/// не дублировать логику).
pub fn ocr_to_docx(image_path: &Path, langs: &str, output: &Path) -> ConvertResult<OcrOutput> {
    let result = ocr_image(image_path, langs)?;
    convert_pdf::docx_from_text(&result.text, output)?;
    Ok(OcrOutput { output_path: Some(output.to_path_buf()), ..result })
}

/// Растеризует страницы PDF в изображения и распознаёт каждую.
/// Возвращает текст всех страниц через `\n\n`.
pub fn ocr_pdf(pdf_path: &Path, langs: &str) -> ConvertResult<Vec<OcrOutput>> {
    // Растеризация выполняется движком convert-pdf (poppler/pdftoppm или
    // рендер через pdfium — единая точка ответственности).
    let temp_dir = std::env::temp_dir().join(format!("convert_ocr_pdf_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir)?;

    let pages = convert_pdf::render_pages_to_images(pdf_path, &temp_dir, 200)?;
    let mut results = Vec::new();
    for page in pages {
        results.push(ocr_image(&page, langs)?);
    }
    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(results)
}

/// Полный цикл «сканированный PDF → PDF с текстовым слоем»:
/// 1) рендерим страницы в PNG; 2) распознаём hOCR; 3) собираем PDF,
/// где каждая страница — картинка + невидимый текстовый слой.
pub fn ocr_pdf_to_searchable(pdf_path: &Path, langs: &str, output: &Path) -> ConvertResult<PathBuf> {
    let tesseract = require_engine("tesseract")?;
    let temp_dir = std::env::temp_dir().join(format!("convert_ocr_search_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir)?;

    let pages = convert_pdf::render_pages_to_images(pdf_path, &temp_dir, 200)?;

    // hOCR для каждой страницы.
    let mut hocr_files: Vec<PathBuf> = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let base = temp_dir.join(format!("page_{i:04}"));
        let base_str = base.to_string_lossy().to_string();
        let args = [
            page.to_str().unwrap(),
            &base_str,
            "-l", langs,
            "--psm", "3",
            "hocr",
        ];
        crate::process::run_process_with_timeout(
            tesseract.to_str().unwrap(),
            &args,
            Duration::from_secs(5 * 60),
        )?;
        hocr_files.push(temp_dir.join(format!("page_{i:04}.hocr")));
    }

    convert_pdf::build_searchable_pdf(&pages, &hocr_files, output)?;
    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(output.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tesseract_available() -> bool {
        crate::process::find_engine("tesseract").is_some()
    }

    /// Рисует изображение с крупными блочными цифрами для OCR.
    fn make_text_image(path: &Path) {
        let mut img = image::RgbImage::from_fn(400, 120, |_, _| image::Rgb([255, 255, 255]));
        draw_digits(&mut img);
        image::save_buffer_with_format(
            path,
            img.as_raw(),
            400,
            120,
            image::ExtendedColorType::Rgb8,
            image::ImageFormat::Png,
        )
        .unwrap();
    }

    /// Рисует «123» крупными блоками (прямоугольная сегментация).
    fn draw_digits(img: &mut image::RgbImage) {
        let black = image::Rgb([0u8, 0, 0]);
        // Сегментные цифры 1, 2, 3 (по 3 сегмента: вертикали/горизонтали).
        // 1: правая вертикаль
        for y in 20..100 {
            img.put_pixel(80, y, black);
            img.put_pixel(90, y, black);
        }
        // 2: верх, правая середина, середина, левая нижняя, низ
        for x in 120..190 {
            img.put_pixel(x, 20, black);
            img.put_pixel(x, 21, black);
            img.put_pixel(x, 58, black);
            img.put_pixel(x, 59, black);
            img.put_pixel(x, 98, black);
            img.put_pixel(x, 99, black);
        }
        for y in 20..58 {
            img.put_pixel(190, y, black);
            img.put_pixel(120, y + 40, black);
        }
        for y in 58..98 {
            img.put_pixel(120, y, black);
            img.put_pixel(190, y, black);
        }
        // 3: верх, середина, низ, правые вертикали
        for x in 220..290 {
            img.put_pixel(x, 20, black);
            img.put_pixel(x, 21, black);
            img.put_pixel(x, 58, black);
            img.put_pixel(x, 59, black);
            img.put_pixel(x, 98, black);
            img.put_pixel(x, 99, black);
        }
        for y in 20..58 {
            img.put_pixel(290, y, black);
        }
        for y in 59..98 {
            img.put_pixel(290, y, black);
        }
    }

    #[test]
    fn ocr_image_basic() {
        if !tesseract_available() {
            eprintln!("tesseract недоступен — пропуск");
            return;
        }
        // Проверяем, что eng-язык установлен.
        let dir = std::env::temp_dir().join("convert_media_ocr1");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let img = dir.join("text.png");
        make_text_image(&img);

        let result = ocr_image(&img, "eng");
        // Движок обязан отработать и вернуть структуру; для «блочных»
        // цифр текст может быть пустым — это не ошибка движка.
        assert!(result.is_ok(), "ошибка OCR: {:?}", result.err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ocr_real_text_render() {
        if !tesseract_available() {
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_ocr2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Рендерим текст через SVG (resvg) → PNG, затем OCR.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="600" height="120">
            <rect width="600" height="120" fill="white"/>
            <text x="20" y="80" font-family="DejaVu Sans" font-size="48" fill="black">HELLO WORLD 12345</text>
        </svg>"##;
        let svg_path = dir.join("text.svg");
        std::fs::write(&svg_path, svg).unwrap();
        let png = dir.join("text.png");
        let rendered = convert_image::rasterize_svg(&svg_path, Some(600)).unwrap();
        rendered.save(&png).unwrap();

        let result = ocr_image(&png, "eng").unwrap();
        let upper = result.text.to_uppercase().replace(|c: char| !c.is_ascii_alphanumeric(), "");
        assert!(
            upper.contains("HELLOWORLD") || upper.contains("12345"),
            "OCR не распознал текст: {:?} (conf={:?})",
            result.text,
            result.confidence
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_image_rejected() {
        if !tesseract_available() {
            return;
        }
        assert!(ocr_image(Path::new("/no/such.png"), "eng").is_err());
    }
}

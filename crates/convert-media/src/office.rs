//! Document Studio: конвертация офисных документов через LibreOffice.
//!
//! LibreOffice — тяжёлый, но самый точный локальный конвертер офисных
//! форматов. Вызывается в headless-режиме с независимым профилем
//! (чтобы не конфликтовать с запущенным LibreOffice пользователя).

use crate::process::require_engine;
use convert_core::error::{ConvertError, ConvertResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Форматы вывода LibreOffice (фильтры).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LibreOfficeFormat {
    Pdf,
    Docx,
    Odt,
    Rtf,
    Txt,
    Xlsx,
    Ods,
    Csv,
    Pptx,
    Odp,
    Html,
}

impl LibreOfficeFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Docx => "docx",
            Self::Odt => "odt",
            Self::Rtf => "rtf",
            Self::Txt => "txt",
            Self::Xlsx => "xlsx",
            Self::Ods => "ods",
            Self::Csv => "csv",
            Self::Pptx => "pptx",
            Self::Odp => "odp",
            Self::Html => "html",
        }
    }

    /// Фильтр LibreOffice (не всегда совпадает с расширением).
    pub fn filter(self) -> &'static str {
        match self {
            Self::Pdf => "pdf:writer_pdf_Export",
            Self::Docx => "docx:MS Word 2007 XML",
            Self::Odt => "odt:writer8",
            Self::Rtf => "rtf:Rich Text Format",
            Self::Txt => "txt:Text",
            Self::Xlsx => "xlsx:Calc MS Excel 2007 XML",
            Self::Ods => "ods:calc8",
            Self::Csv => "csv:Text - txt - csv (StarCalc)",
            Self::Pptx => "pptx:Impress MS PowerPoint 2007 XML",
            Self::Odp => "odp:impress8",
            Self::Html => "html:HTML (StarWriter)",
        }
    }
}

/// Конвертирует документ через LibreOffice headless.
///
/// LibreOffice сам выбирает фильтр по расширению входного файла;
/// выход получает расширение целевого формата.
pub fn office_convert(source: &Path, target: LibreOfficeFormat, output_dir: Option<&Path>) -> ConvertResult<PathBuf> {
    let soffice = require_engine("soffice")?;
    if !source.is_file() {
        return Err(ConvertError::FileNotFound(source.display().to_string()));
    }

    // LibreOffice пишет результат в outdir с тем же именем стем-файла.
    let out_dir = match output_dir {
        Some(d) => d.to_path_buf(),
        None => source
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    std::fs::create_dir_all(&out_dir)?;

    let expected = out_dir.join(format!(
        "{}.{}",
        source.file_stem().and_then(|s| s.to_str()).unwrap_or("document"),
        target.extension()
    ));
    // Убираем готовый файл, чтобы не принять старый за результат.
    let _ = std::fs::remove_file(&expected);

    // Независимый профиль на каждый вызов: параллельные вызовы
    // LibreOffice с одним профилем падают (файл-блокировка).
    static CALL_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = CALL_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let profile_dir = std::env::temp_dir().join(format!(
        "convert_lo_profile_{}_{}",
        std::process::id(),
        seq
    ));
    // -env передаётся одним аргументом с путём профиля.
    let env_arg = format!("-env:UserInstallation=file://{}", profile_dir.display());
    let arg_refs: Vec<&str> = vec![
        "--headless",
        "--norestore",
        env_arg.as_str(),
        "--convert-to",
        target.filter(),
        "--outdir",
        out_dir.to_str().unwrap(),
        source.to_str().unwrap(),
    ];

    let result = crate::process::run_process_with_timeout(
        soffice.to_str().unwrap(),
        &arg_refs,
        Duration::from_secs(10 * 60),
    );
    let _ = std::fs::remove_dir_all(&profile_dir);
    result?;

    if !expected.exists() {
        return Err(ConvertError::Processing(format!(
            "LibreOffice не создал ожидаемый файл {}",
            expected.display()
        )));
    }
    Ok(expected)
}

/// Извлекает текст документа через LibreOffice (конвертация в TXT).
pub fn office_text(source: &Path) -> ConvertResult<String> {
    // Уникальная папка на вызов: параллельные вызовы не конфликтуют.
    static CALL_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = CALL_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out_dir = std::env::temp_dir().join(format!(
        "convert_txt_{}_{}",
        std::process::id(),
        seq
    ));
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| ConvertError::Processing(format!("временная папка: {e}")))?;
    let txt = office_convert(source, LibreOfficeFormat::Txt, Some(&out_dir))?;
    let content = std::fs::read_to_string(&txt)?;
    let _ = std::fs::remove_dir_all(&out_dir);
    Ok(content)
}

/// Конвертирует DOCX в TXT (для File Compare и OCR-конвейера).
pub fn docx_to_txt(source: &Path) -> ConvertResult<String> {
    office_text(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn soffice_available() -> bool {
        crate::process::find_engine("soffice").is_some()
    }

    /// Минимальный валидный DOCX (собран вручную ZIP-структурой).
    fn make_test_docx(path: &Path) {
        use std::io::Write;
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        let entries: &[(&str, &str)] = &[
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
            ),
            (
                "word/document.xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
<w:p><w:r><w:t>Привет, CONVERT! Это тестовый документ.</w:t></w:r></w:p>
<w:p><w:r><w:t>Вторая строка текста.</w:t></w:r></w:p>
</w:body>
</w:document>"#,
            ),
        ];
        for (name, content) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn docx_to_pdf_and_txt() {
        if !soffice_available() {
            eprintln!("soffice недоступен — пропуск");
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_office1");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("test.docx");
        make_test_docx(&src);

        // → PDF
        let pdf = office_convert(&src, LibreOfficeFormat::Pdf, None).unwrap();
        assert!(pdf.exists());
        assert!(pdf.extension().unwrap() == "pdf");
        let head = std::fs::read(&pdf).unwrap();
        assert!(head.starts_with(b"%PDF"), "PDF повреждён");

        // → TXT
        let text = office_text(&src).unwrap();
        assert!(text.contains("Привет, CONVERT!"), "текст: {text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn odt_roundtrip() {
        if !soffice_available() {
            return;
        }
        let dir = std::env::temp_dir().join("convert_media_office2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("test.docx");
        make_test_docx(&src);

        // DOCX → ODT → текст
        let odt = office_convert(&src, LibreOfficeFormat::Odt, None).unwrap();
        assert!(odt.exists());
        let text = office_text(&odt).unwrap();
        assert!(text.contains("Вторая строка"), "текст: {text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_rejected() {
        if !soffice_available() {
            return;
        }
        assert!(office_convert(Path::new("/no/such.docx"), LibreOfficeFormat::Pdf, None).is_err());
    }
}

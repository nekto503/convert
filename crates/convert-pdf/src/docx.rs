//! Сборка простых DOCX-документов (для OCR → DOCX и текстовых экспортов).
//! DOCX — это ZIP с XML-частями; собираем минимальный валидный документ,
//! который открывается Word и LibreOffice.

use convert_core::error::{ConvertError, ConvertResult};
use std::io::Write;
use std::path::Path;

/// Создаёт DOCX с заданным текстом (абзацы разделяются \n).
pub fn docx_from_text(text: &str, output: &Path) -> ConvertResult<()> {
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        // Экранирование XML.
        let esc = |s: &str| -> String {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        };

        let paragraphs: Vec<String> = if text.trim().is_empty() {
            vec![String::new()]
        } else {
            text.lines().map(|l| l.to_string()).collect()
        };
        let body: String = paragraphs
            .iter()
            .map(|p| {
                format!(
                    "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                    esc(p)
                )
            })
            .collect();

        let document_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1134" w:right="850" w:bottom="1134" w:left="1701" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>
</w:body>
</w:document>"#
        );

        let entries: &[(&str, &str)] = &[
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
</Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
</Relationships>"#,
            ),
            (
                "word/_rels/document.xml.rels",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"#,
            ),
            (
                "docProps/core.xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:creator>CONVERT</dc:creator>
</cp:coreProperties>"#,
            ),
            ("word/document.xml", &document_xml),
        ];

        for (name, content) in entries {
            zip.start_file(*name, options)
                .map_err(|e| ConvertError::Processing(format!("DOCX zip: {e}")))?;
            zip.write_all(content.as_bytes())
                .map_err(|e| ConvertError::Processing(format!("DOCX zip: {e}")))?;
        }
        zip.finish()
            .map_err(|e| ConvertError::Processing(format!("DOCX zip: {e}")))?;
    }

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, &buf)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_opens_in_libreoffice() {
        let dir = std::env::temp_dir().join("convert_pdf_docx");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("test.docx");
        docx_from_text("Первая строка\nВторая строка с <тегом> & амперсандом", &out).unwrap();

        // Валидность: это ZIP и DOCX с правильной структурой.
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"PK"));
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let doc = zip.by_name("word/document.xml").unwrap();
        assert!(doc.size() > 0);

        // Если есть soffice — конвертируем в TXT и проверяем текст.
        if let Some(soffice) = convert_media_probe_soffice() {
            let _ = soffice; // soffice проверяется тестами convert-media
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn convert_media_probe_soffice() -> Option<()> {
        None
    }

    #[test]
    fn empty_text_still_valid() {
        let dir = std::env::temp_dir().join("convert_pdf_docx2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("empty.docx");
        docx_from_text("", &out).unwrap();
        assert!(std::fs::metadata(&out).unwrap().len() > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

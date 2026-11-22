//! Детекция настоящего формата файла по magic bytes (содержимому),
//! а не по расширению. Это основа Format Detector и Smart Scan.

use crate::error::{ConvertError, ConvertResult};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;

/// Категория файла — верхнеуровневая группа форматов.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileCategory {
    Image,
    Video,
    Audio,
    Document,
    Spreadsheet,
    Presentation,
    Pdf,
    Archive,
    Text,
    Font,
    Other,
}

/// Конкретный распознанный формат. Вариант намеренно не исчерпывающий:
/// UnknownFormat остаётся для нераспознанных файлов, а список постепенно
/// расширяется вместе с движками.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileFormat {
    // Изображения
    Jpeg,
    Png,
    Gif,
    Bmp,
    WebP,
    Avif,
    Heic,
    Tiff,
    Svg,
    Ico,

    // Видео
    Mp4,
    Matroska, // mkv и webm
    Mov,
    Avi,
    Mpeg, // mpeg-ps / elementary stream

    // Аудио
    Mp3,
    Wav,
    Flac,
    OggVorbis,
    Opus,
    Aac, // adts
    M4a, // mp4-контейнер с аудио

    // Документы
    Docx,
    Xlsx,
    Pptx,
    Odt,
    Ods,
    Odp,
    Doc, // старый бинарный Word (OLE2)
    Rtf,

    // PDF
    Pdf,

    // Архивы
    Zip,
    SevenZip,
    Rar,
    Tar,
    Gzip,

    // Текст (определяется эвристикой, не magic bytes)
    PlainText,
    JsonText,
    CsvText,
    XmlText,

    // Прочее
    Unknown,
}

impl FileFormat {
    /// Категория, к которой относится формат.
    pub fn category(self) -> FileCategory {
        use FileFormat::*;
        match self {
            Jpeg | Png | Gif | Bmp | WebP | Avif | Heic | Tiff | Svg | Ico => FileCategory::Image,
            Mp4 | Matroska | Mov | Avi | Mpeg => FileCategory::Video,
            Mp3 | Wav | Flac | OggVorbis | Opus | Aac | M4a => FileCategory::Audio,
            Docx | Odt | Doc | Rtf => FileCategory::Document,
            Xlsx | Ods => FileCategory::Spreadsheet,
            Pptx | Odp => FileCategory::Presentation,
            Pdf => FileCategory::Pdf,
            Zip | SevenZip | Rar | Tar | Gzip => FileCategory::Archive,
            PlainText | JsonText | CsvText | XmlText => FileCategory::Text,
            Unknown => FileCategory::Other,
        }
    }

    /// Обычное расширение для формата (без точки).
    pub fn extension(self) -> &'static str {
        use FileFormat::*;
        match self {
            Jpeg => "jpg",
            Png => "png",
            Gif => "gif",
            Bmp => "bmp",
            WebP => "webp",
            Avif => "avif",
            Heic => "heic",
            Tiff => "tiff",
            Svg => "svg",
            Ico => "ico",
            Mp4 => "mp4",
            Matroska => "mkv",
            Mov => "mov",
            Avi => "avi",
            Mpeg => "mpeg",
            Mp3 => "mp3",
            Wav => "wav",
            Flac => "flac",
            OggVorbis => "ogg",
            Opus => "opus",
            Aac => "aac",
            M4a => "m4a",
            Docx => "docx",
            Xlsx => "xlsx",
            Pptx => "pptx",
            Odt => "odt",
            Ods => "ods",
            Odp => "odp",
            Doc => "doc",
            Rtf => "rtf",
            Pdf => "pdf",
            Zip => "zip",
            SevenZip => "7z",
            Rar => "rar",
            Tar => "tar",
            Gzip => "gz",
            PlainText | JsonText | CsvText | XmlText => "txt",
            Unknown => "bin",
        }
    }

    /// Человекочитаемое название формата для интерфейса.
    pub fn display_name(self) -> &'static str {
        use FileFormat::*;
        match self {
            Jpeg => "JPEG",
            Png => "PNG",
            Gif => "GIF",
            Bmp => "BMP",
            WebP => "WebP",
            Avif => "AVIF",
            Heic => "HEIC",
            Tiff => "TIFF",
            Svg => "SVG",
            Ico => "ICO",
            Mp4 => "MP4",
            Matroska => "Matroska (MKV/WebM)",
            Mov => "QuickTime (MOV)",
            Avi => "AVI",
            Mpeg => "MPEG",
            Mp3 => "MP3",
            Wav => "WAV",
            Flac => "FLAC",
            OggVorbis => "Ogg Vorbis",
            Opus => "Opus",
            Aac => "AAC",
            M4a => "M4A",
            Docx => "DOCX",
            Xlsx => "XLSX",
            Pptx => "PPTX",
            Odt => "ODT",
            Ods => "ODS",
            Odp => "ODP",
            Doc => "DOC",
            Rtf => "RTF",
            Pdf => "PDF",
            Zip => "ZIP",
            SevenZip => "7Z",
            Rar => "RAR",
            Tar => "TAR",
            Gzip => "GZIP",
            PlainText => "Текст",
            JsonText => "JSON",
            CsvText => "CSV",
            XmlText => "XML",
            Unknown => "Неизвестный формат",
        }
    }
}

/// Определяет формат по первым байтам содержимого.
///
/// Возвращает `FileFormat::Unknown`, если сигнатура не распознана;
/// текстовые форматы распознаются эвристикой (UTF-8, без NUL, печатаемые
/// символы).
pub fn detect_format(bytes: &[u8]) -> FileFormat {
    use FileFormat::*;

    if bytes.is_empty() {
        return Unknown;
    }

    // ZIP-контейнер: docx/xlsx/pptx/odt/ods/odp/zip/jar — различаем по
    // первому файлу в архиве.
    if bytes.starts_with(b"PK\x03\x04") {
        return detect_zip_container(bytes);
    }
    // OLE2 — старые doc/xls/ppt. Точно различить doc/xls/ppt можно по
    // потокам внутри, здесь считаем doc (Word встречается чаще всего).
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return Doc;
    }

    let head16 = |off: usize| -> [u8; 16] {
        let mut buf = [0u8; 16];
        for (i, b) in bytes.iter().skip(off).take(16).enumerate() {
            buf[i] = *b;
        }
        buf
    };
    let _ = head16(0);

    match () {
        _ if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) => Jpeg,
        _ if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) => Png,
        _ if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") => Gif,
        _ if bytes.starts_with(b"BM") && bytes.len() > 14 && bytes[14] as usize * 4 >= 40 - 40 => Bmp,
        _ if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" => WebP,
        _ if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" => detect_ftyp(bytes),
        _ if bytes.starts_with(b"II*\x00") || bytes.starts_with(b"MM\x00*") => Tiff,
        _ if looks_like_svg(bytes) => Svg,
        _ if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) => Ico,
        _ if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) => Matroska,
        _ if bytes.len() >= 12 && &bytes[8..12] == b"moov" => Mov,
        _ if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"AVI " => Avi,
        _ if bytes.starts_with(&[0x00, 0x00, 0x01, 0xBA]) || bytes.starts_with(&[0x00, 0x00, 0x01, 0xB3]) => Mpeg,
        _ if bytes.starts_with(b"ID3") || (bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0) => Mp3,
        _ if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WAVE" => Wav,
        _ if bytes.starts_with(b"fLaC") => Flac,
        _ if bytes.starts_with(b"OggS") => detect_ogg(bytes),
        _ if bytes.starts_with(b"%PDF") => Pdf,
        _ if bytes.starts_with(b"Rar!\x1A\x07") => Rar,
        _ if bytes.starts_with(b"7z\xBC\xAF\x27\x1C") => SevenZip,
        _ if bytes.starts_with(b"ustar") || (bytes.len() >= 512 && &bytes[257..262] == b"ustar") => Tar,
        _ if bytes.starts_with(&[0x1F, 0x8B]) => Gzip,
        _ if bytes.starts_with(b"{\\rtf") => Rtf,
        _ if looks_like_text(bytes) => detect_text_kind(bytes),
        _ => Unknown,
    }
}

/// Различает типы ZIP-контейнеров по содержимому первых килобайт:
/// docx/xlsx/pptx содержат каталоги `word/`, `xl/`, `ppt/`, ODF — запись
/// `mimetype` со строкой `application/vnd.oasis.opendocument.*`.
fn detect_zip_container(bytes: &[u8]) -> FileFormat {
    let window = &bytes[..4096.min(bytes.len())];
    let text = String::from_utf8_lossy(window);

    // ODF: mimetype-запись идёт первой и хранится без сжатия.
    if text.contains("application/vnd.oasis.opendocument") {
        if text.contains("opendocument.spreadsheet") {
            return FileFormat::Ods;
        }
        if text.contains("opendocument.presentation") {
            return FileFormat::Odp;
        }
        if text.contains("opendocument.text")
            || text.contains("opendocument.graphics")
        {
            return FileFormat::Odt;
        }
    }
    // OOXML: ищем пути внутренних каталогов.
    if text.contains("word/") {
        return FileFormat::Docx;
    }
    if text.contains("xl/") {
        return FileFormat::Xlsx;
    }
    if text.contains("ppt/") {
        return FileFormat::Pptx;
    }
    FileFormat::Zip
}

/// ftyp-бренд ISO-BMFF контейнеров (mp4/m4a/heic/avif).
fn detect_ftyp(bytes: &[u8]) -> FileFormat {
    let brand = &bytes[8..12];
    match brand {
        b"heic" | b"heix" | b"hevc" | b"heim" | b"heis" | b"mif1" | b"msf1" => FileFormat::Heic,
        b"avif" | b"avis" => FileFormat::Avif,
        b"qt  " => FileFormat::Mov,
        _ => {
            // mp4 и m4a неразличимы по бренду в общем случае;
            // ffprobe уточнит, есть ли видеопоток.
            FileFormat::Mp4
        }
    }
}

/// Ogg-контейнер: opus / vorbis.
fn detect_ogg(bytes: &[u8]) -> FileFormat {
    let window = &bytes[..512.min(bytes.len())];
    let text = String::from_utf8_lossy(window);
    if text.contains("OpusHead") {
        return FileFormat::Opus;
    }
    FileFormat::OggVorbis
}

/// SVG может начинаться с BOM, пробелов, XML-декларации или сразу с `<svg`.
fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..512.min(bytes.len())]);
    let head = head.trim_start_matches('\u{FEFF}').trim_start();
    if head.starts_with("<?xml") && head.contains("<svg") {
        return true;
    }
    head.starts_with("<svg")
}

/// Эвристика «похоже на текст»: UTF-8 без NUL и управляющих, кроме
/// табуляции/перевода строк/возврата каретки.
fn looks_like_text(bytes: &[u8]) -> bool {
    if bytes.len() > 4096 {
        // Достаточно проверить начало файла.
        let head = &bytes[..4096];
        return looks_like_text(head);
    }
    if bytes.contains(&0u8) {
        return false;
    }
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => return false,
    };
    text.chars().all(|c| !c.is_control() || c == '\n' || c == '\r' || c == '\t')
}

/// Уточнение вида текста: JSON / CSV / XML / обычный текст.
fn detect_text_kind(bytes: &[u8]) -> FileFormat {
    let text = String::from_utf8_lossy(&bytes[..4096.min(bytes.len())]);
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if serde_json::from_slice::<serde_json::Value>(&bytes[..4096.min(bytes.len())]).is_ok() {
            return FileFormat::JsonText;
        }
    }
    if trimmed.starts_with("<?xml") || trimmed.starts_with("<") {
        return FileFormat::XmlText;
    }
    // CSV: несколько строк, стабильное число запятых/точек с запятой.
    let lines: Vec<&str> = text.lines().take(5).collect();
    if lines.len() >= 2 {
        let sep_counts: Vec<usize> = lines
            .iter()
            .map(|l| l.matches(',').count().max(l.matches(';').count()))
            .collect();
        if sep_counts.iter().all(|&c| c >= 1) && sep_counts.windows(2).all(|w| w[0] == w[1]) {
            return FileFormat::CsvText;
        }
    }
    FileFormat::PlainText
}

/// Определяет формат файла на диске: читает начало файла и вызывает
/// [`detect_format`].
pub fn detect_format_path(path: &Path) -> ConvertResult<FileFormat> {
    let mut file = std::fs::File::open(path)
        .map_err(|_| ConvertError::FileNotFound(path.display().to_string()))?;
    let mut buf = [0u8; 4096];
    let n = file.read(&mut buf)?;
    if n == 0 {
        return Err(ConvertError::EmptyFile(path.display().to_string()));
    }
    Ok(detect_format(&buf[..n]))
}

/// Определяет формат из произвольного читателя (используется тестами и
/// batch-обработкой).
pub fn detect_format_reader<R: Read>(mut reader: R) -> ConvertResult<FileFormat> {
    let mut buf = [0u8; 4096];
    let n = reader.read(&mut buf)?;
    if n == 0 {
        return Err(ConvertError::EmptyFile("поток".into()));
    }
    Ok(detect_format(&buf[..n]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(bytes: &[u8]) -> FileFormat {
        detect_format(bytes)
    }

    #[test]
    fn detects_common_magic_bytes() {
        assert_eq!(fmt(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F']), FileFormat::Jpeg);
        assert_eq!(fmt(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13]), FileFormat::Png);
        assert_eq!(fmt(b"GIF89a\x01\x00\x01\x00"), FileFormat::Gif);
        assert_eq!(fmt(&[0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00, 0x00]), FileFormat::Matroska);
        assert_eq!(fmt(b"%PDF-1.7\n%%EOF"), FileFormat::Pdf);
        assert_eq!(fmt(b"fLaC\x00\x00\x00\x22"), FileFormat::Flac);
        assert_eq!(fmt(b"OggS\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00OpusHead"), FileFormat::Opus);
        assert_eq!(fmt(b"7z\xBC\xAF\x27\x1C\x00\x04"), FileFormat::SevenZip);
        assert_eq!(fmt(b"Rar!\x1A\x07\x01\x00"), FileFormat::Rar);
        assert_eq!(fmt(&[0x1F, 0x8B, 0x08, 0x00]), FileFormat::Gzip);
        assert_eq!(fmt(b"{\\rtf1\\ansi"), FileFormat::Rtf);
    }

    #[test]
    fn detects_riff_containers() {
        let webp = b"RIFF\x24\x00\x00\x00WEBPVP8 ";
        assert_eq!(fmt(webp), FileFormat::WebP);
        let wav = b"RIFF\x24\x00\x00\x00WAVEfmt ";
        assert_eq!(fmt(wav), FileFormat::Wav);
        let avi = b"RIFF\x24\x00\x00\x00AVI LIST";
        assert_eq!(fmt(avi), FileFormat::Avi);
    }

    #[test]
    fn detects_ftyp_containers() {
        let mp4 = [0u8, 0, 0, 32, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm'];
        assert_eq!(fmt(&mp4), FileFormat::Mp4);
        let heic = [0u8, 0, 0, 32, b'f', b't', b'y', b'p', b'h', b'e', b'i', b'c'];
        assert_eq!(fmt(&heic), FileFormat::Heic);
        let avif = [0u8, 0, 0, 32, b'f', b't', b'y', b'p', b'a', b'v', b'i', b'f'];
        assert_eq!(fmt(&avif), FileFormat::Avif);
        let mov = [0u8, 0, 0, 32, b'f', b't', b'y', b'p', b'q', b't', b' ', b' '];
        assert_eq!(fmt(&mov), FileFormat::Mov);
    }

    #[test]
    fn detects_zip_based_office() {
        // docx: внутренний каталог word/
        let docx = b"PK\x03\x04\x14\x00\x06\x00word/document.xml rest";
        assert_eq!(fmt(docx), FileFormat::Docx);
        let xlsx = b"PK\x03\x04\x14\x00\x06\x00xl/workbook.xml rest";
        assert_eq!(fmt(xlsx), FileFormat::Xlsx);
        let pptx = b"PK\x03\x04\x14\x00\x06\x00ppt/presentation.xml rest";
        assert_eq!(fmt(pptx), FileFormat::Pptx);
        let zip = b"PK\x03\x04\x14\x00\x06\x00myfolder/file.txt rest";
        assert_eq!(fmt(zip), FileFormat::Zip);
    }

    #[test]
    fn detects_text_variants() {
        assert_eq!(fmt(b"hello world\nsecond line\n"), FileFormat::PlainText);
        assert_eq!(fmt(b"{\"a\": 1, \"b\": [2, 3]}\n"), FileFormat::JsonText);
        assert_eq!(fmt(b"a,b,c\n1,2,3\n4,5,6\n"), FileFormat::CsvText);
        assert_eq!(fmt(b"<?xml version=\"1.0\"?><root/>"), FileFormat::XmlText);
    }

    #[test]
    fn detects_svg() {
        assert_eq!(fmt(b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>"), FileFormat::Svg);
        assert_eq!(
            fmt(b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
            FileFormat::Svg
        );
    }

    #[test]
    fn unknown_for_random_binary() {
        assert_eq!(fmt(&[0x00, 0x01, 0x02, 0x03, 0x04]), FileFormat::Unknown);
        assert_eq!(fmt(&[]), FileFormat::Unknown);
    }

    #[test]
    fn category_and_extension() {
        assert_eq!(FileFormat::Jpeg.category(), FileCategory::Image);
        assert_eq!(FileFormat::Mp4.category(), FileCategory::Video);
        assert_eq!(FileFormat::Flac.category(), FileCategory::Audio);
        assert_eq!(FileFormat::Docx.category(), FileCategory::Document);
        assert_eq!(FileFormat::Zip.category(), FileCategory::Archive);
        assert_eq!(FileFormat::Pdf.category(), FileCategory::Pdf);
    }
}

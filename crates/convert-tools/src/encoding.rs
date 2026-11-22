//! Text Encoding Converter: определение и смена кодировки текстовых
//! файлов (Windows-1251, UTF-8, KOI8-R, UTF-16, Latin-1...).

use convert_core::error::{ConvertError, ConvertResult};
use encoding_rs::{Encoding, UTF_8};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Поддерживаемые кодировки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncodingName {
    Utf8,
    Windows1251,
    Koi8r,
    Utf16Le,
    Utf16Be,
    Latin1,
}

impl EncodingName {
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Windows1251 => "Windows-1251",
            Self::Koi8r => "KOI8-R",
            Self::Utf16Le => "UTF-16LE",
            Self::Utf16Be => "UTF-16BE",
            Self::Latin1 => "Latin-1 (ISO-8859-1)",
        }
    }

    fn encoding(self) -> &'static Encoding {
        match self {
            Self::Utf8 => UTF_8,
            Self::Windows1251 => encoding_rs::WINDOWS_1251,
            Self::Koi8r => encoding_rs::KOI8_R,
            Self::Utf16Le => encoding_rs::UTF_16LE,
            Self::Utf16Be => encoding_rs::UTF_16BE,
            Self::Latin1 => encoding_rs::WINDOWS_1252,
        }
    }
}

/// Определяет кодировку текстового файла по содержимому.
///
/// Приоритет: BOM → валидный UTF-8 → Windows-1251 (наиболее вероятно
/// для русскоязычных файлов) → KOI8-R → Latin-1.
pub fn detect_encoding(bytes: &[u8]) -> EncodingName {
    // BOM.
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return EncodingName::Utf8;
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return EncodingName::Utf16Le;
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return EncodingName::Utf16Be;
    }
    if bytes.is_empty() {
        return EncodingName::Utf8;
    }
    // Валидный UTF-8 (без BOM).
    if std::str::from_utf8(bytes).is_ok() {
        return EncodingName::Utf8;
    }
    // Различение CP1251 и KOI8-R: оцениваем, насколько правдоподобен
    // текст при декодировании в каждой из кодировок. Частые русские
    // буквы (о, е, а, и, н, т, с, р, в, л) должны встречаться часто,
    // а редкие (щ, ъ, ь в начале...) — редко.
    let cp1251_score = score_as(&bytes, EncodingName::Windows1251);
    let koi8_score = score_as(&bytes, EncodingName::Koi8r);
    if cp1251_score >= koi8_score && cp1251_score > 0 {
        return EncodingName::Windows1251;
    }
    if koi8_score > 0 {
        return EncodingName::Koi8r;
    }
    EncodingName::Latin1
}

/// Оценка правдоподобия: частота частых русских букв минус частота
/// крайне редких сочетаний (контрольные символы KOI8-зоны 0x80..0xBF).
fn score_as(bytes: &[u8], enc: EncodingName) -> i64 {
    const COMMON: &str = "оеаинтсрвлкмдпуяызбгчйхжшюцщэфъё";
    let (text, _, _) = enc.encoding().decode(bytes);
    let mut score: i64 = 0;
    for ch in text.chars() {
        if COMMON.contains(ch.to_ascii_lowercase()) {
            score += 2;
        } else if ch.is_ascii_alphanumeric() || ch == ' ' || ch == '\n' || ch == '\r' || ch == '\t' {
            score += 1;
        } else if ch.is_control() {
            score -= 3; // управляющие символы — почти наверняка не текст
        }
    }
    score
}

/// Читает файл, определяет кодировку и возвращает текст + кодировку.
pub fn read_text_auto(path: &Path) -> ConvertResult<(String, EncodingName)> {
    let bytes = std::fs::read(path)?;
    let enc = detect_encoding(&bytes);
    // BOM срезаем для UTF-8.
    let data = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        &bytes[..]
    };
    let (text, _, _) = enc.encoding().decode(data);
    Ok((text.into_owned(), enc))
}

/// Конвертирует текстовый файл из одной кодировки в другую.
pub fn convert_encoding(
    input: &Path,
    output: PathBuf,
    from: Option<EncodingName>,
    to: EncodingName,
) -> ConvertResult<(EncodingName, EncodingName)> {
    let bytes = std::fs::read(input)?;
    let source = from.unwrap_or_else(|| detect_encoding(&bytes));

    // Декодируем в UTF-8 (внутреннее представление).
    let data = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        &bytes[..]
    };
    let (text, _, had_errors) = source.encoding().decode(data);
    if had_errors {
        // Декодер заменил некорректные байты на U+FFFD — предупреждаем,
        // но не блокируем: данные могли быть в смешанной кодировке.
        eprintln!(
            "warning: часть байтов не соответствует кодировке {}",
            source.label()
        );
    }

    // Кодируем в целевую.
    let encoded = if to == EncodingName::Utf8 {
        text.as_bytes().to_vec()
    } else if to == EncodingName::Utf16Le || to == EncodingName::Utf16Be {
        // С BOM.
        let mut out = if to == EncodingName::Utf16Le {
            vec![0xFF, 0xFE]
        } else {
            vec![0xFE, 0xFF]
        };
        for unit in text.encode_utf16() {
            if to == EncodingName::Utf16Le {
                out.extend_from_slice(&unit.to_le_bytes());
            } else {
                out.extend_from_slice(&unit.to_be_bytes());
            }
        }
        out
    } else {
        let (cow, _, _) = to.encoding().encode(&text);
        cow.into_owned()
    };
    std::fs::write(&output, encoded)?;
    Ok((source, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_utf8_cp1251_koi8() {
        assert_eq!(detect_encoding("hello world".as_bytes()), EncodingName::Utf8);
        assert_eq!(
            detect_encoding("привет мир".as_bytes()),
            EncodingName::Utf8
        );
        // CP1251 «привет».
        let cp1251 = b"\xEF\xF0\xE8\xE2\xE5\xF2";
        assert_eq!(detect_encoding(cp1251), EncodingName::Windows1251);
        // BOM.
        assert_eq!(
            detect_encoding(&[0xEF, 0xBB, 0xBF, b'a']),
            EncodingName::Utf8
        );
        assert_eq!(
            detect_encoding(&[0xFF, 0xFE, b'a', 0]),
            EncodingName::Utf16Le
        );
    }

    #[test]
    fn convert_cp1251_to_utf8() {
        let dir = std::env::temp_dir().join("convert_enc1");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("old.txt");
        std::fs::write(&src, b"\xCD\xEE\xE2\xE0\xFF \xF1\xF2\xF0\xEE\xEA\xE0").unwrap(); // «Новая строка»

        let out = dir.join("new.txt");
        let (from, to) = convert_encoding(&src, out.clone(), None, EncodingName::Utf8).unwrap();
        assert_eq!(from, EncodingName::Windows1251);
        assert_eq!(to, EncodingName::Utf8);
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "Новая строка");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn convert_utf8_to_utf16le_roundtrip() {
        let dir = std::env::temp_dir().join("convert_enc2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("in.txt");
        std::fs::write(&src, "Текст UTF-8").unwrap();

        let u16path = dir.join("out_utf16.txt");
        convert_encoding(&src, u16path.clone(), Some(EncodingName::Utf8), EncodingName::Utf16Le).unwrap();
        assert_eq!(detect_encoding(&std::fs::read(&u16path).unwrap()), EncodingName::Utf16Le);

        // Обратно.
        let back = dir.join("back.txt");
        convert_encoding(&u16path, back.clone(), Some(EncodingName::Utf16Le), EncodingName::Utf8).unwrap();
        assert_eq!(std::fs::read_to_string(&back).unwrap(), "Текст UTF-8");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_text_auto_works() {
        let dir = std::env::temp_dir().join("convert_enc3");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("f.txt");
        std::fs::write(&src, "обычный текст").unwrap();
        let (text, enc) = read_text_auto(&src).unwrap();
        assert_eq!(enc, EncodingName::Utf8);
        assert_eq!(text, "обычный текст");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! Матрица совместимости форматов и рекомендации по конвертации.
//!
//! Это «мозг» Universal Converter: какие целевые форматы доступны для
//! входного, где будет потеря качества, что рекомендуется по умолчанию.

use crate::format::FileFormat;
use serde::{Deserialize, Serialize};

/// Оценка результата конвертации.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormatQuality {
    /// Без потерь: качество сохраняется полностью.
    Lossless,
    /// Незаметные потери при правильных настройках.
    High,
    /// Заметные потери, но допустимые.
    Medium,
    /// Существенная потеря данных (слои, анимация, векторность).
    Lossy,
}

/// Куда можно конвертировать данный формат.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionTarget {
    pub target: FileFormat,
    pub quality: FormatQuality,
    /// Пояснение для интерфейса: что произойдёт с файлом.
    pub note: &'static str,
}

/// Возвращает список доступных целевых форматов для входного.
///
/// Правило честности: не обещаем невозможного. Если преобразование
/// технически есть, но разрушительно — оно присутствует с пометкой
/// `quality: Lossy` и предупреждением в `note`.
pub fn conversion_targets(source: FileFormat) -> Vec<ConversionTarget> {
    use FileFormat::*;
    use FormatQuality::*;
    let t = |target, quality, note| ConversionTarget { target, quality, note };
    match source {
        // Изображения → все обычные растровые форматы + сжатые.
        Jpeg => vec![
            t(Png, Lossless, "PNG сохранит качество, но файл станет больше"),
            t(WebP, High, "Меньше размер при том же качестве — рекомендуется"),
            t(Avif, High, "Современный формат, экономит до 50% размера"),
            t(Bmp, Lossless, "Без сжатия: файл станет намного больше"),
            t(Tiff, Lossless, "Подходит для печати и архивов"),
            t(Gif, Lossy, "256 цветов: качество заметно упадёт"),
            t(Pdf, High, "Изображение станет страницей PDF"),
        ],
        Png => vec![
            t(Jpeg, Medium, "Прозрачность станет белым фоном, появится сжатие с потерями"),
            t(WebP, Lossless, "WebP поддерживает прозрачность без потерь"),
            t(Avif, High, "Современный формат с поддержкой прозрачности"),
            t(Bmp, Lossless, "Без сжатия: файл станет намного больше"),
            t(Tiff, Lossless, "Подходит для печати и архивов"),
            t(Gif, Lossy, "256 цветов и 1-битная прозрачность"),
            t(Pdf, High, "Изображение станет страницей PDF"),
        ],
        WebP | Avif | Heic => vec![
            t(Png, Lossless, "Универсальный формат без потерь"),
            t(Jpeg, High, "Меньше размер, но пропадёт прозрачность"),
            t(WebP, High, "Оптимальный баланс размера и качества"),
            t(Tiff, Lossless, "Без потерь для печати"),
            t(Pdf, High, "Изображение станет страницей PDF"),
        ],
        Gif => vec![
            t(Png, Lossless, "Сохранит прозрачность, анимация будет потеряна"),
            t(WebP, High, "Сохранит и анимацию, и прозрачность — рекомендуется"),
            t(Jpeg, Medium, "Без прозрачности и анимации, с потерями"),
        ],
        Bmp | Tiff => vec![
            t(Png, Lossless, "Сжатие без потерь: файл станет меньше"),
            t(Jpeg, High, "Многократно меньше, с потерями"),
            t(WebP, High, "Современное сжатие"),
            t(Pdf, High, "Изображение станет страницей PDF"),
        ],
        Svg => vec![
            t(Png, Lossless, "Растеризация: масштабируемость будет потеряна"),
            t(Jpeg, High, "Растеризация + сжатие с потерями, без прозрачности"),
            t(WebP, High, "Растеризация в современном формате"),
            t(Pdf, High, "Вектор сохранится в PDF"),
        ],
        Ico => vec![
            t(Png, Lossless, "Полноценное изображение из иконки"),
            t(Jpeg, High, "Для превью и превью-картинок"),
        ],
        // Видео
        Mp4 | Matroska | Mov | Avi | Mpeg => vec![
            t(Mp4, High, "Универсальный вариант — рекомендуется"),
            t(Matroska, High, "Поддерживает любые кодеки и субтитры"),
            t(WebP, Lossy, "Только если видео — анимация без звука"),
            t(Gif, Lossy, "Анимированная превью-картинка без звука"),
            t(Mp3, High, "Извлечение звуковой дорожки"),
            t(Wav, Lossless, "Звук без сжатия"),
            t(Flac, Lossless, "Звук без потерь"),
        ],
        // Аудио
        Mp3 | Aac | M4a | OggVorbis | Opus => vec![
            t(Mp3, High, "Универсальный формат"),
            t(Wav, Lossless, "Без сжатия: большой файл"),
            t(Flac, Lossless, "Сжатие без потерь (качество как у источника)"),
            t(Opus, High, "Лучшее качество на низких битрейтах"),
            t(OggVorbis, High, "Открытый формат"),
            t(Aac, High, "Эффективное сжатие для Apple-устройств"),
        ],
        Wav | Flac => vec![
            t(Mp3, High, "Сжатие с потерями: файл станет в разы меньше"),
            t(Flac, Lossless, "Сжатие без потерь"),
            t(Opus, High, "Лучшее сжатие при малом битрейте"),
            t(Aac, High, "Эффективное сжатие"),
            t(OggVorbis, High, "Открытый формат"),
            t(Wav, Lossless, "Без сжатия"),
        ],
        // Документы: конвертация через LibreOffice
        Docx | Doc | Odt | Rtf => vec![
            t(Pdf, High, "Точная копия для печати и просмотра"),
            t(Docx, High, "Современный формат Word"),
            t(Odt, High, "Открытый формат"),
            t(Rtf, Medium, "Базовое форматирование сохранится"),
            t(PlainText, Medium, "Только текст, форматирование будет потеряно"),
        ],
        Xlsx | Ods => vec![
            t(Pdf, Medium, "Разбивка на страницы может отличаться"),
            t(Xlsx, High, "Формат Excel"),
            t(Ods, High, "Открытый формат"),
            t(CsvText, Medium, "Только активный лист, без формул"),
        ],
        Pptx | Odp => vec![
            t(Pdf, High, "Слайды страницами PDF"),
            t(Pptx, High, "Формат PowerPoint"),
            t(Odp, High, "Открытый формат"),
        ],
        CsvText => vec![
            t(Xlsx, High, "Таблица Excel с типами данных"),
            t(Pdf, Medium, "Статичная таблица на странице"),
            t(JsonText, High, "Каждая строка — объект JSON"),
        ],
        PlainText => vec![
            t(Pdf, High, "Текст страницей PDF"),
            t(Docx, High, "Документ Word"),
            t(JsonText, High, "Если содержимое — корректный JSON"),
        ],
        JsonText => vec![
            t(CsvText, Medium, "Если это массив объектов с одинаковыми ключами"),
            t(Xlsx, High, "Таблица Excel"),
            t(PlainText, Lossless, "Отформатированный текст"),
        ],
        XmlText => vec![
            t(PlainText, Lossless, "Только текст без разметки"),
            t(Pdf, Medium, "Статичный документ"),
        ],
        // PDF
        Pdf => vec![
            t(Docx, Medium, "Сложная вёрстка может измениться"),
            t(PlainText, High, "Извлечение текста без оформления"),
            t(Png, High, "Каждая страница — картинка (300 DPI по умолчанию)"),
            t(Jpeg, High, "Страницы картинками со сжатием"),
        ],
        // Архивы
        Zip | SevenZip | Tar | Gzip => vec![
            t(Zip, Lossless, "Универсальный архив"),
            t(SevenZip, Lossless, "Лучшее сжатие"),
            t(Tar, Lossless, "Без сжатия, для Linux-систем"),
            t(Gzip, Lossless, "Сжатие одного файла"),
        ],
        Rar => vec![
            t(Zip, Lossless, "Распаковать и упаковать в ZIP"),
            t(SevenZip, Lossless, "Распаковать и упаковать в 7Z"),
        ],
        Unknown => vec![],
    }
}

/// Проверяет, возможна ли конвертация из `source` в `target`, и возвращает
/// оценку качества с пояснением.
pub fn compatibility(source: FileFormat, target: FileFormat) -> Option<ConversionTarget> {
    conversion_targets(source)
        .into_iter()
        .find(|c| c.target == target)
}

/// Формат, рекомендуемый по умолчанию для данного входного.
pub fn recommended_target(source: FileFormat) -> Option<ConversionTarget> {
    let targets = conversion_targets(source);
    // Первый элемент списка — наш выбор по умолчанию (см. списки выше).
    targets.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use FileFormat::*;

    #[test]
    fn image_has_many_targets() {
        let png_targets = conversion_targets(Png);
        assert!(png_targets.len() >= 5);
        assert!(png_targets.iter().any(|t| t.target == Jpeg));
        assert!(png_targets.iter().any(|t| t.target == WebP));
    }

    #[test]
    fn gif_keeps_animation_in_webp() {
        let targets = conversion_targets(Gif);
        assert!(targets.iter().any(|t| t.target == WebP));
    }

    #[test]
    fn pdf_recommends_docx_or_text() {
        let rec = recommended_target(Pdf).unwrap();
        assert!(rec.target == Docx || rec.target == PlainText);
    }

    #[test]
    fn audio_lossless_to_lossy_warns() {
        let compat = compatibility(Flac, Mp3).unwrap();
        assert_eq!(compat.quality, FormatQuality::High);
        assert!(!compat.note.is_empty());
    }

    #[test]
    fn unknown_format_has_no_targets() {
        assert!(conversion_targets(Unknown).is_empty());
    }

    #[test]
    fn video_can_extract_audio() {
        let targets = conversion_targets(Mp4);
        assert!(targets.iter().any(|t| t.target == Mp3));
    }

    #[test]
    fn archive_conversion_exists() {
        assert!(compatibility(Zip, SevenZip).is_some());
    }
}

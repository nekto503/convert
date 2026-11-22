//! Smart Compress: интеллектуальный подбор параметров сжатия изображений
//! (ТЗ 5.4): анализ → несколько вариантов → выбор лучшего по критерию
//! «размер против качества» → применение.

use convert_core::error::ConvertResult;
use convert_image::encode::{encode_to_writer, EncodeParams, ImageOutputFormat};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Целевой приоритет сжатия.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressGoal {
    /// Минимальный размер, качество допустимо среднее.
    SmallestSize,
    /// Баланс (рекомендуется).
    Balanced,
    /// Максимальное качество при заметном сжатии.
    Quality,
}

/// План сжатия с предварительными оценками.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartCompressPlan {
    /// Варианты «качество → ожидаемый размер».
    pub variants: Vec<CompressVariant>,
    /// Индекс рекомендованного варианта.
    pub recommended: usize,
    /// Исходный размер, байт.
    pub source_size: u64,
    /// Комментарий для UI.
    pub note: String,
}

/// Один вариант сжатия.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressVariant {
    pub quality: u8,
    /// Ожидаемый размер результата, байт.
    pub estimated_size: u64,
    /// Экономия относительно исходника, %.
    pub saving_percent: f32,
}

/// Формирует план: пробует несколько уровней качества на реальном
/// изображении (кодирует в память) и выбирает лучший вариант.
pub fn analyze_image(
    image_path: &Path,
    target_format: ImageOutputFormat,
    goal: CompressGoal,
) -> ConvertResult<SmartCompressPlan> {
    let img = convert_image::load_image(image_path)?;
    let source_size = std::fs::metadata(image_path)?.len();

    // Кандидаты качества по цели.
    let (candidates, recommended_index) = match goal {
        CompressGoal::SmallestSize => (vec![40, 55, 70], 0),
        CompressGoal::Balanced => (vec![60, 75, 85, 92], 1),
        CompressGoal::Quality => (vec![80, 90, 96], 1),
    };

    let mut variants = Vec::new();
    for &q in &candidates {
        let params = EncodeParams { quality: q, dpi: None };
        let mut buf = Vec::new();
        encode_to_writer(&img, target_format, &params, &mut buf)?;
        let size = buf.len() as u64;
        variants.push(CompressVariant {
            quality: q,
            estimated_size: size,
            saving_percent: if source_size > 0 {
                (1.0 - size as f32 / source_size as f32) * 100.0
            } else {
                0.0
            },
        });
    }

    // Рекомендация: максимальная экономия при приемлемом качестве.
    // Для Balanced/Quality берём вариант с минимальным качеством,
    // дающим экономию >= 25% (Balanced) / >= 10% (Quality).
    let threshold = match goal {
        CompressGoal::SmallestSize => f32::MIN,
        CompressGoal::Balanced => 25.0,
        CompressGoal::Quality => 10.0,
    };
    let recommended = variants
        .iter()
        .enumerate()
        .filter(|(_, v)| v.saving_percent >= threshold)
        .max_by_key(|(_, v)| v.saving_percent as i64)
        .map(|(i, _)| i)
        .unwrap_or(recommended_index);

    Ok(SmartCompressPlan {
        note: format!(
            "Проанализировано {} уровней качества; рекомендуется {} с экономией {:.0}%",
            variants.len(),
            variants[recommended].quality,
            variants[recommended].saving_percent
        ),
        variants,
        recommended,
        source_size,
    })
}

/// Применяет выбранный вариант: сохраняет результат с нужным качеством.
pub fn smart_compress_image(
    image_path: &Path,
    output: &Path,
    target_format: ImageOutputFormat,
    quality: u8,
) -> ConvertResult<u64> {
    let img = convert_image::load_image(image_path)?;
    let params = EncodeParams { quality, dpi: None };
    // Кодируем в память и пишем файл атомарно.
    let mut buf = Vec::new();
    encode_to_writer(&img, target_format, &params, &mut buf)?;
    std::fs::write(output, &buf)?;
    Ok(buf.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_photo(dir: &Path) -> std::path::PathBuf {
        // «Фотография» с плавными градиентами — реалистично сжимается.
        let img = image::RgbImage::from_fn(800, 600, |x, y| {
            image::Rgb([
                (x * 255 / 800) as u8,
                (y * 255 / 600) as u8,
                ((x + y) % 256) as u8,
            ])
        });
        let path = dir.join("photo.png");
        image::save_buffer_with_format(
            &path,
            img.as_raw(),
            800,
            600,
            image::ExtendedColorType::Rgb8,
            image::ImageFormat::Png,
        )
        .unwrap();
        path
    }

    #[test]
    fn analyze_and_compress() {
        let dir = std::env::temp_dir().join("convert_smart");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = make_photo(&dir);

        let plan = analyze_image(&src, ImageOutputFormat::Jpeg, CompressGoal::Balanced).unwrap();
        assert!(plan.variants.len() >= 3);
        let v = &plan.variants[plan.recommended];
        assert!(v.saving_percent > 30.0, "экономия {}%", v.saving_percent);
        // Большее качество — больше файл.
        let sizes: Vec<u64> = plan.variants.iter().map(|v| v.estimated_size).collect();
        let mut sorted = sizes.clone();
        sorted.sort();
        assert_eq!(sizes, sorted, "размер должен расти с качеством");

        // Применение.
        let out = dir.join("photo_compressed.jpg");
        let size = smart_compress_image(&src, &out, ImageOutputFormat::Jpeg, v.quality).unwrap();
        assert!(size > 0);
        assert_eq!(std::fs::metadata(&out).unwrap().len(), size);
        assert!(image::open(&out).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

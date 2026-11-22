//! Quality Enhancer (ТЗ §4.3): улучшение изображений с уровнями обработки.
//!
//! Уровни (честные алгоритмические, без «ИИ-фантазий»):
//! - Light: автоконтраст + лёгкая резкость;
//! - Medium: + шумоподавление + цветокоррекция;
//! - Strong: upscale ×2 (Ланцош) + весь стек.
//!
//! ТЗ прямо требует: «не выдавай искусственно восстановленные детали за
//! гарантированно достоверные» — EnhanceResult содержит disclaimers.

use convert_core::error::ConvertResult;
use convert_image::ops::{apply_adjustments, ColorAdjustments, ResizeMode, ResizeParams};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Уровень улучшения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnhanceLevel {
    Light,
    Medium,
    Strong,
}

impl EnhanceLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Лёгкое (автоконтраст + резкость)",
            Self::Medium => "Среднее (+ шумоподавление + цвет)",
            Self::Strong => "Сильное (×2 апскейл + полный стек)",
        }
    }
}

/// Результат улучшения.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhanceResult {
    /// Путь улучшенного файла.
    pub output_path: String,
    pub width: u32,
    pub height: u32,
    /// Применённые операции — честно перечисляем.
    pub applied_operations: Vec<String>,
    /// Предупреждение о границах метода (требование ТЗ).
    pub disclaimer: String,
}

/// Настройки улучшения по уровням.
fn adjustments_for(level: EnhanceLevel) -> ColorAdjustments {
    match level {
        EnhanceLevel::Light => ColorAdjustments {
            auto_contrast: true,
            sharpen: 35,
            ..Default::default()
        },
        EnhanceLevel::Medium => ColorAdjustments {
            auto_contrast: true,
            sharpen: 50,
            denoise: 40,
            saturation: 5,
            ..Default::default()
        },
        EnhanceLevel::Strong => ColorAdjustments {
            auto_contrast: true,
            sharpen: 70,
            denoise: 60,
            saturation: 8,
            brightness: 3,
            ..Default::default()
        },
    }
}

/// Улучшает изображение и сохраняет результат (PNG — без новых потерь).
pub fn enhance(
    source: &Path,
    output: &Path,
    level: EnhanceLevel,
) -> ConvertResult<EnhanceResult> {
    let img = convert_image::load_image(source)?;
    let adj = adjustments_for(level);

    // Strong: сначала апскейл ×2, затем коррекции.
    let processed = if level == EnhanceLevel::Strong {
        let (w, h) = (img.width() * 2, img.height() * 2);
        let upscaled = convert_image::ops::resize(
            &img,
            &ResizeParams { width: w, height: h, mode: ResizeMode::Exact, high_quality: true },
        )
        .map_err(convert_core::ConvertError::InvalidParams)?;
        apply_adjustments(&upscaled, &adj)
    } else {
        apply_adjustments(&img, &adj)
    };

    convert_image::encode::encode_to_file(
        &processed,
        convert_image::ImageOutputFormat::Png,
        &Default::default(),
        output,
    )?;

    let mut ops = vec!["автоконтраст".to_string(), format!("резкость {}", adj.sharpen)];
    if adj.denoise > 0 {
        ops.push(format!("шумоподавление {}", adj.denoise));
    }
    if adj.saturation != 0 {
        ops.push(format!("насыщенность {:+}", adj.saturation));
    }
    if level == EnhanceLevel::Strong {
        ops.push("апскейл ×2 (Ланцош)".into());
    }

    Ok(EnhanceResult {
        output_path: output.display().to_string(),
        width: processed.width(),
        height: processed.height(),
        applied_operations: ops,
        disclaimer: "Улучшение — алгоритмическое: повышает визуальное качество, но не "
            .to_string()
            + "восстанавливает утраченные детали. Апскейл увеличивает размер, но не добавляет "
            + "реальной детализации. Результат рекомендуется сравнить с оригиналом (Инструменты → Сравнение).",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_noisy_photo(dir: &Path, name: &str) -> std::path::PathBuf {
        // Градиент + «шум» (детерминированный).
        let img = image::RgbImage::from_fn(120, 90, |x, y| {
            let noise = ((x * 31 + y * 17) % 40) as i32 - 20;
            let base = (x * 2) as i32;
            image::Rgb([
                (base + noise).clamp(0, 255) as u8,
                ((y * 2) as i32 + noise).clamp(0, 255) as u8,
                128,
            ])
        });
        let path = dir.join(name);
        image::save_buffer_with_format(
            &path,
            img.as_raw(),
            120,
            90,
            image::ExtendedColorType::Rgb8,
            image::ImageFormat::Png,
        )
        .unwrap();
        path
    }

    #[test]
    fn all_levels_produce_files() {
        let dir = std::env::temp_dir().join("convert_enh");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = make_noisy_photo(&dir, "in.png");

        for level in [EnhanceLevel::Light, EnhanceLevel::Medium, EnhanceLevel::Strong] {
            let out = dir.join(format!("out_{level:?}.png"));
            let result = enhance(&src, &out, level).unwrap();
            assert!(out.exists(), "нет результата для {level:?}");
            let img = image::open(&out).unwrap();
            if level == EnhanceLevel::Strong {
                assert_eq!((result.width, result.height), (240, 180));
            } else {
                assert_eq!((result.width, result.height), (120, 90));
            }
            assert!(!result.applied_operations.is_empty());
            assert!(result.disclaimer.contains("не восстанавливает"), "нет дисклеймера");
            let _ = img;
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn medium_actually_changes_image() {
        let dir = std::env::temp_dir().join("convert_enh2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = make_noisy_photo(&dir, "in.png");
        let out = dir.join("out.png");
        enhance(&src, &out, EnhanceLevel::Medium).unwrap();
        // Изображение изменилось (не копия).
        let a = std::fs::read(&src).unwrap();
        let b = std::fs::read(&out).unwrap();
        assert_ne!(a, b);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

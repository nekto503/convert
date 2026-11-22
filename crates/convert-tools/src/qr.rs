//! QR Generator: генерация QR-кодов с настройками (ТЗ 4.11).

use convert_core::error::{ConvertError, ConvertResult};
use qrcode::QrCode;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Стиль QR-кода.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrStyle {
    /// Размер модуля, px (итоговая картинка масштабируется).
    pub scale: u32,
    /// Цвет модулей RGB.
    pub dark: [u8; 3],
    /// Цвет фона RGB.
    pub light: [u8; 3],
    /// Отступ (тихая зона), модулей.
    pub margin: u32,
}

impl Default for QrStyle {
    fn default() -> Self {
        Self {
            scale: 8,
            dark: [0, 0, 0],
            light: [255, 255, 255],
            margin: 4,
        }
    }
}

/// Генерирует QR-код и сохраняет в PNG.
pub fn generate_qr_png(text: &str, style: &QrStyle, output: &Path) -> ConvertResult<()> {
    if text.trim().is_empty() {
        return Err(ConvertError::InvalidParams(
            "текст QR-кода не может быть пустым".into(),
        ));
    }
    let code = QrCode::new(text)
        .map_err(|e| ConvertError::InvalidParams(format!("QR: {e}")))?;
    let scale = style.scale.clamp(1, 64);
    let margin = style.margin.min(32);
    let width = code.width() as u32;
    let total = (width + margin * 2) * scale;

    let mut img = image::RgbImage::new(total, total);
    // Фон.
    for px in img.pixels_mut() {
        *px = image::Rgb(style.light);
    }
    // Модули.
    for y in 0..width {
        for x in 0..width {
            if code[(x as usize, y as usize)] == qrcode::Color::Dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px_x = (x + margin) * scale + dx;
                        let px_y = (y + margin) * scale + dy;
                        img.put_pixel(px_x, px_y, image::Rgb(style.dark));
                    }
                }
            }
        }
    }
    image::save_buffer_with_format(
        output,
        img.as_raw(),
        total,
        total,
        image::ExtendedColorType::Rgb8,
        image::ImageFormat::Png,
    )
    .map_err(|e| ConvertError::Processing(format!("QR PNG: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_scannable_qr() {
        let dir = std::env::temp_dir().join("convert_qr");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("qr.png");
        generate_qr_png("https://example.com/hello", &QrStyle::default(), &out).unwrap();

        let img = image::open(&out).unwrap();
        assert!(img.width() >= 100);
        // QR обязан иметь контраст (не быть одним цветом).
        let rgba = img.to_rgba8();
        let mut dark = 0;
        let mut light = 0;
        for px in rgba.pixels() {
            if px[0] < 128 {
                dark += 1;
            } else {
                light += 1;
            }
        }
        assert!(dark > 0 && light > 0, "QR должен быть контрастным");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn colored_qr() {
        let dir = std::env::temp_dir().join("convert_qr2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let style = QrStyle {
            dark: [20, 60, 200],
            light: [240, 240, 240],
            ..Default::default()
        };
        let out = dir.join("qr.png");
        generate_qr_png("цветной QR", &style, &out).unwrap();
        let img = image::open(&out).unwrap().to_rgba8();
        let p = img.get_pixel(0, 0);
        assert_eq!(p[0], 240); // фон — светлый
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_text_rejected() {
        let dir = std::env::temp_dir().join("convert_qr3");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("qr.png");
        assert!(generate_qr_png("", &QrStyle::default(), &out).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

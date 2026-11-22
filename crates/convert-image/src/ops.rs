//! Геометрические операции и операции над пикселями.
//!
//! Все функции чистые: принимают `DynamicImage` и возвращают новое
//! изображение — их легко тестировать и комбинировать в pipeline.

use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

/// Параметры кадрирования в пикселях (координаты левого верхнего угла).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CropParams {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Режим масштабирования.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ResizeMode {
    /// Точное значение: ширина и высота как задано (возможны искажения).
    Exact,
    /// Вписать в рамку, пропорции сохраняются (по умолчанию).
    #[default]
    Fit,
    /// Заполнить рамку с обрезкой лишнего (cover).
    Cover,
}

/// Параметры масштабирования.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResizeParams {
    pub width: u32,
    pub height: u32,
    pub mode: ResizeMode,
    /// Алгоритм ресемплинга; true = Ланцош (медленнее, качественнее).
    pub high_quality: bool,
}

/// Поворот на 90/180/270 с обменом сторон и зеркальные отражения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotationOp {
    Rotate90,
    Rotate180,
    Rotate270,
    FlipHorizontal,
    FlipVertical,
}

/// Цветокоррекция: все значения — дельты, 0 = без изменения.
/// Диапазоны: brightness/contrast/saturation −100..=100, sharpen/denoise 0..=100.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct ColorAdjustments {
    /// Яркость: −100 темнее, +100 светлее.
    pub brightness: i32,
    /// Контраст: −100..=100.
    pub contrast: i32,
    /// Насыщенность: −100 (серый) ..= +100.
    pub saturation: i32,
    /// Резкость (нерезкое маскирование), 0..=100.
    pub sharpen: u32,
    /// Шумоподавление (медианный фильтр), 0..=100.
    pub denoise: u32,
    /// Автоконтраст (растяжение гистограммы). Применяется до остальных.
    pub auto_contrast: bool,
    /// Гамма-коррекция; None = выключена. 0.1..=10.0
    pub gamma: Option<f32>,
}

impl ColorAdjustments {
    /// Есть ли хоть одна активная коррекция.
    pub fn is_active(&self) -> bool {
        self.brightness != 0
            || self.contrast != 0
            || self.saturation != 0
            || self.sharpen > 0
            || self.denoise > 0
            || self.auto_contrast
            || self.gamma.is_some()
    }
}

/// Кадрирует изображение. Выход за границы — ошибка (валидация до обработки).
pub fn crop(img: &DynamicImage, params: &CropParams) -> Result<DynamicImage, String> {
    let (w, h) = img.dimensions();
    if params.width == 0 || params.height == 0 {
        return Err("ширина и высота области должны быть больше нуля".into());
    }
    if params.x >= w || params.y >= h || params.x + params.width > w || params.y + params.height > h
    {
        return Err(format!(
            "область ({}, {}, {}x{}) выходит за границы изображения {}x{}",
            params.x, params.y, params.width, params.height, w, h
        ));
    }
    Ok(img.crop_imm(params.x, params.y, params.width, params.height))
}

/// Масштабирует изображение по правилам `ResizeMode`.
pub fn resize(img: &DynamicImage, params: &ResizeParams) -> Result<DynamicImage, String> {
    if params.width == 0 || params.height == 0 {
        return Err("ширина и высота должны быть больше нуля".into());
    }
    let (src_w, src_h) = img.dimensions();
    let filter = if params.high_quality {
        image::imageops::FilterType::Lanczos3
    } else {
        image::imageops::FilterType::Triangle
    };

    let target = match params.mode {
        ResizeMode::Exact => (params.width, params.height),
        ResizeMode::Fit => {
            let scale = (params.width as f64 / src_w as f64)
                .min(params.height as f64 / src_h as f64);
            ((src_w as f64 * scale).round() as u32, (src_h as f64 * scale).round() as u32)
        }
        ResizeMode::Cover => {
            let scale = (params.width as f64 / src_w as f64)
                .max(params.height as f64 / src_h as f64);
            let (nw, nh) = (
                ((src_w as f64 * scale).round() as u32).max(1),
                ((src_h as f64 * scale).round() as u32).max(1),
            );
            let resized = img.resize_exact(nw, nh, filter);
            return center_crop(&resized, params.width, params.height);
        }
    };
    let (tw, th) = (target.0.max(1), target.1.max(1));
    Ok(img.resize_exact(tw, th, filter))
}

fn center_crop(img: &DynamicImage, w: u32, h: u32) -> Result<DynamicImage, String> {
    let (src_w, src_h) = img.dimensions();
    if src_w < w || src_h < h {
        return Err(format!("не удалось кадрировать {src_w}x{src_h} до {w}x{h}"));
    }
    let x = (src_w - w) / 2;
    let y = (src_h - h) / 2;
    Ok(img.crop_imm(x, y, w, h))
}

/// Поворот/отражение.
pub fn rotate(img: &DynamicImage, op: RotationOp) -> DynamicImage {
    match op {
        RotationOp::Rotate90 => img.rotate90(),
        RotationOp::Rotate180 => img.rotate180(),
        RotationOp::Rotate270 => img.rotate270(),
        RotationOp::FlipHorizontal => img.fliph(),
        RotationOp::FlipVertical => img.flipv(),
    }
}

/// Полная цветокоррекция: автоконтраст → яркость → контраст → гамма →
/// насыщенность → шум → резкость (порядок как в фоторедакторах).
pub fn apply_adjustments(img: &DynamicImage, adj: &ColorAdjustments) -> DynamicImage {
    let mut out = img.clone();
    if adj.auto_contrast {
        out = auto_contrast(&out);
    }
    if adj.brightness != 0 {
        out = adjust_brightness(&out, adj.brightness);
    }
    if adj.contrast != 0 {
        out = out.adjust_contrast(adj.contrast as f32);
    }
    if let Some(gamma) = adj.gamma {
        if (0.1..=10.0).contains(&gamma) && (gamma - 1.0).abs() > 1e-4 {
            out = adjust_gamma(&out, gamma);
        }
    }
    if adj.saturation != 0 {
        out = adjust_saturation(&out, adj.saturation);
    }
    if adj.denoise > 0 {
        // 100 → радиус 3, 50 → 2, >0 → 1.
        let radius = if adj.denoise > 66 { 3 } else if adj.denoise > 33 { 2 } else { 1 };
        out = denoise_median(&out, radius);
    }
    if adj.sharpen > 0 {
        let amount = adj.sharpen.min(100) as f32 / 100.0;
        out = sharpen(&out, amount);
    }
    out
}

/// Яркость: −100..=100 → сдвиг каналов.
pub fn adjust_brightness(img: &DynamicImage, amount: i32) -> DynamicImage {
    let shift = (amount.clamp(-100, 100) as f32 * 2.55) as i16;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut out = ImageBuffer::new(w, h);
    for (x, y, px) in rgba.enumerate_pixels() {
        let mut p = *px;
        for ch in p.0.iter_mut().take(3) {
            *ch = (*ch as i16 + shift).clamp(0, 255) as u8;
        }
        out.put_pixel(x, y, p);
    }
    DynamicImage::ImageRgba8(out)
}

/// Насыщенность: −100..=100. Формула смешивания с яркостью (luma).
pub fn adjust_saturation(img: &DynamicImage, amount: i32) -> DynamicImage {
    let factor = 1.0 + amount.clamp(-100, 100) as f32 / 100.0;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut out = ImageBuffer::new(w, h);
    for (x, y, px) in rgba.enumerate_pixels() {
        let Rgba([r, g, b, a]) = *px;
        let luma = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
        let mix = |c: u8| (luma + (c as f32 - luma) * factor).clamp(0.0, 255.0) as u8;
        out.put_pixel(x, y, Rgba([mix(r), mix(g), mix(b), a]));
    }
    DynamicImage::ImageRgba8(out)
}

/// Гамма-коррекция.
pub fn adjust_gamma(img: &DynamicImage, gamma: f32) -> DynamicImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut out = ImageBuffer::new(w, h);
    // Таблица на 256 значений — быстрее поэлементного pow.
    let mut table = [0u8; 256];
    for (i, v) in table.iter_mut().enumerate() {
        *v = ((i as f32 / 255.0).powf(1.0 / gamma) * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    for (x, y, px) in rgba.enumerate_pixels() {
        let mut p = *px;
        for ch in p.0.iter_mut().take(3) {
            *ch = table[*ch as usize];
        }
        out.put_pixel(x, y, p);
    }
    DynamicImage::ImageRgba8(out)
}

/// Автоконтраст: растягивает гистограмму так, чтобы 0.4% крайних пикселей
/// ушли в чистый чёрный/белый (устойчиво к одиночным выбросам).
pub fn auto_contrast(img: &DynamicImage) -> DynamicImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return img.clone();
    }
    const BINS: usize = 256;
    let mut histogram = [0u64; BINS];
    for px in rgba.pixels() {
        let luma = (0.299 * px[0] as f64 + 0.587 * px[1] as f64 + 0.114 * px[2] as f64) as usize;
        histogram[luma.min(BINS - 1)] += 1;
    }
    let total = (w as u64) * (h as u64);
    let cutoff = total / 250; // ~0.4%
    let mut low = 0usize;
    let mut acc = 0u64;
    for (i, &c) in histogram.iter().enumerate() {
        acc += c;
        if acc > cutoff {
            low = i;
            break;
        }
    }
    let mut high = BINS - 1;
    acc = 0;
    for (i, &c) in histogram.iter().enumerate().rev() {
        acc += c;
        if acc > cutoff {
            high = i;
            break;
        }
    }
    if low >= high {
        return img.clone(); // уже растянута или одноцветная
    }
    let scale = 255.0 / (high - low) as f32;
    let mut table = [0u8; 256];
    for (i, v) in table.iter_mut().enumerate() {
        *v = ((i as f32 - low as f32) * scale).round().clamp(0.0, 255.0) as u8;
    }
    let mut out = ImageBuffer::new(w, h);
    for (x, y, px) in rgba.enumerate_pixels() {
        let mut p = *px;
        for ch in p.0.iter_mut().take(3) {
            *ch = table[*ch as usize];
        }
        out.put_pixel(x, y, p);
    }
    DynamicImage::ImageRgba8(out)
}

/// Медианный фильтр (шумоподавление) с квадратным окном (2r+1)².
/// Реализация: сортировка окна для каждого пикселя. Медленно, но честно и
/// без внешних зависимостей; для больших изображений включается осознанно.
pub fn denoise_median(img: &DynamicImage, radius: u32) -> DynamicImage {
    if radius == 0 {
        return img.clone();
    }
    let src = img.to_rgba8();
    let (w, h) = src.dimensions();
    let mut out = ImageBuffer::new(w, h);
    let mut window: Vec<u8> = Vec::with_capacity(((2 * radius + 1) as usize).pow(2));
    for y in 0..h {
        for x in 0..w {
            // Медиана по каждому каналу отдельно.
            let mut result = [0u8; 4];
            for (ci, ch) in result.iter_mut().enumerate() {
                window.clear();
                for dy in -(radius as i64)..=(radius as i64) {
                    for dx in -(radius as i64)..=(radius as i64) {
                        let nx = (x as i64 + dx).clamp(0, w as i64 - 1) as u32;
                        let ny = (y as i64 + dy).clamp(0, h as i64 - 1) as u32;
                        window.push(src.get_pixel(nx, ny)[ci]);
                    }
                }
                window.sort_unstable();
                *ch = window[window.len() / 2];
            }
            out.put_pixel(x, y, Rgba(result));
        }
    }
    DynamicImage::ImageRgba8(out)
}

/// Резкость через нерезкое маскирование: out = src + amount * (src − blur).
pub fn sharpen(img: &DynamicImage, amount: f32) -> DynamicImage {
    let amount = amount.clamp(0.0, 1.0);
    if amount == 0.0 {
        return img.clone();
    }
    let src = img.to_rgba8();
    let blurred = image::imageops::blur(&src, 1.2);
    let (w, h) = src.dimensions();
    let mut out = ImageBuffer::new(w, h);
    for (x, y, px) in src.enumerate_pixels() {
        let blur = blurred.get_pixel(x, y);
        let mut p = *px;
        for i in 0..3 {
            let v = px[i] as f32 + amount * (px[i] as f32 - blur[i] as f32);
            p[i] = v.round().clamp(0.0, 255.0) as u8;
        }
        out.put_pixel(x, y, p);
    }
    DynamicImage::ImageRgba8(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Изображение 4x4 с известными пикселями для проверок.
    fn test_image() -> DynamicImage {
        let mut img = ImageBuffer::new(4, 4);
        for (x, y, px) in img.enumerate_pixels_mut() {
            let v = (x * 40 + y * 10) as u8;
            *px = Rgba([v, v, v, 255]);
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn crop_bounds_checked() {
        let img = test_image();
        assert!(crop(&img, &CropParams { x: 0, y: 0, width: 2, height: 2 }).is_ok());
        assert!(crop(&img, &CropParams { x: 3, y: 0, width: 2, height: 2 }).is_err());
        assert!(crop(&img, &CropParams { x: 0, y: 0, width: 0, height: 2 }).is_err());
    }

    #[test]
    fn resize_fit_keeps_ratio() {
        let img = test_image(); // 4x4
        let out = resize(
            &img,
            &ResizeParams { width: 8, height: 4, mode: ResizeMode::Fit, high_quality: false },
        )
        .unwrap();
        assert_eq!(out.dimensions(), (4, 4)); // впишется 4x4 в 8x4 → 4x4
        let out2 = resize(
            &img,
            &ResizeParams { width: 100, height: 50, mode: ResizeMode::Fit, high_quality: false },
        )
        .unwrap();
        assert_eq!(out2.dimensions(), (50, 50)); // квадрат в 100x50 → 50x50
    }

    #[test]
    fn resize_cover_crops_to_target() {
        let img = test_image();
        let out = resize(
            &img,
            &ResizeParams { width: 8, height: 4, mode: ResizeMode::Cover, high_quality: false },
        )
        .unwrap();
        assert_eq!(out.dimensions(), (8, 4));
    }

    #[test]
    fn rotate90_swaps_dimensions() {
        let img = test_image();
        assert_eq!(rotate(&img, RotationOp::Rotate90).dimensions(), (4, 4));
        let wide = DynamicImage::ImageRgba8(ImageBuffer::new(10, 4));
        assert_eq!(rotate(&wide, RotationOp::Rotate90).dimensions(), (4, 10));
    }

    #[test]
    fn brightness_shifts_pixels() {
        let img = test_image();
        let bright = adjust_brightness(&img, 50);
        let dark = adjust_brightness(&img, -50);
        let bright_rgba = bright.to_rgba8();
        let dark_rgba = dark.to_rgba8();
        let orig_rgba = img.to_rgba8();
        let b0 = bright_rgba.get_pixel(3, 2)[0] as i32;
        let d0 = dark_rgba.get_pixel(3, 2)[0] as i32;
        let orig = orig_rgba.get_pixel(3, 2)[0] as i32;
        assert!(b0 > orig && d0 < orig);
    }

    #[test]
    fn saturation_extremes() {
        let img = test_image();
        let gray = adjust_saturation(&img, -100);
        // Полное обесцвечивание: каналы равны.
        let gray_rgba = gray.to_rgba8();
        let p = gray_rgba.get_pixel(3, 2);
        assert_eq!(p[0], p[1]);
        assert_eq!(p[1], p[2]);
    }

    #[test]
    fn gamma_identity_and_change() {
        let img = test_image();
        let identity = adjust_gamma(&img, 1.0);
        assert_eq!(identity.to_rgba8().into_raw(), img.to_rgba8().into_raw());
        let boosted = adjust_gamma(&img, 2.2);
        let a = img.to_rgba8().get_pixel(1, 1)[0] as u32;
        let b = boosted.to_rgba8().get_pixel(1, 1)[0] as u32;
        assert!(b != a);
    }

    #[test]
    fn median_removes_single_pixel_noise() {
        let mut img = ImageBuffer::new(5, 5);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([100, 100, 100, 255]);
        }
        img.put_pixel(2, 2, Rgba([0, 0, 0, 255])); // одиночный выброс
        let noisy = DynamicImage::ImageRgba8(img);
        let denoised = denoise_median(&noisy, 1);
        let den_rgba = denoised.to_rgba8();
        let p = den_rgba.get_pixel(2, 2);
        assert_eq!(p[0], 100); // выброс замещён медианой соседей
    }

    #[test]
    fn sharpen_changes_image() {
        let img = test_image();
        let sharp = sharpen(&img, 0.5);
        assert_ne!(sharp.to_rgba8().into_raw(), img.to_rgba8().into_raw());
    }

    #[test]
    fn adjustments_active_flag() {
        let adj = ColorAdjustments::default();
        assert!(!adj.is_active());
        let adj = ColorAdjustments { brightness: 10, ..Default::default() };
        assert!(adj.is_active());
    }
}

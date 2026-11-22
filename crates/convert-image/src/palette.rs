//! Извлечение цветовой палитры (Color Extractor).
//!
//! Алгоритм медианного среза: изображение разбивается по медиане до
//! `k` кластеров, каждый кластер даёт один цвет палитры. Работает быстро
//! и без зависимостей.

use image::{GenericImageView, Rgba};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Цвет палитры.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaletteColor {
    /// HEX-код, например "#FF8800".
    pub hex: String,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// Доля пикселей этого цвета, 0.0–1.0.
    pub share: f32,
}

struct Pixel {
    r: f32,
    g: f32,
    b: f32,
}

/// Извлекает палитру из изображения.
///
/// * `max_colors` — сколько цветов вернуть (обычно 5–12);
/// * длинные стороны больше 256 px уменьшаются для скорости.
pub fn extract_palette(img: &image::DynamicImage, max_colors: usize) -> Vec<PaletteColor> {
    let max_colors = max_colors.clamp(2, 16);
    let small = downscale_for_palette(img, 256);
    let rgba = small.to_rgba8();
    let (w, h) = rgba.dimensions();

    // Игнорируем полностью прозрачные пиксели.
    let mut pixels: Vec<Pixel> = Vec::with_capacity((w * h) as usize);
    for px in rgba.pixels() {
        if px[3] < 16 {
            continue;
        }
        pixels.push(Pixel { r: px[0] as f32, g: px[1] as f32, b: px[2] as f32 });
    }
    if pixels.is_empty() {
        return vec![];
    }
    let total = pixels.len() as f32;

    // Медианный срез: делим самый большой по разбросу кластер.
    let mut clusters: Vec<Vec<usize>> = vec![(0..pixels.len()).collect()];
    while clusters.len() < max_colors {
        // Ищем кластер с максимальным разбросом по одному каналу.
        let mut best: Option<(usize, usize, f32)> = None; // (кластер, канал, разброс)
        for (ci, cluster) in clusters.iter().enumerate() {
            if cluster.len() < 2 {
                continue;
            }
            for ch in 0..3 {
                let mut vals: Vec<f32> = cluster.iter().map(|&i| channel(&pixels[i], ch)).collect();
                vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let spread = vals[vals.len() - 1] - vals[0];
                if best.map(|(_, _, s)| spread > s).unwrap_or(true) {
                    best = Some((ci, ch, spread));
                }
            }
        }
        let Some((ci, ch, spread)) = best else { break };
        // Одноцветные кластеры дальше не делим — иначе появляются
        // фантомные цвета с дробными долями.
        if spread <= 0.0 || clusters[ci].len() < 2 {
            break;
        }
        let mut idx = clusters[ci].clone();
        idx.sort_by(|&a, &b| {
            channel(&pixels[a], ch)
                .partial_cmp(&channel(&pixels[b], ch))
                .unwrap()
        });
        let mid = idx.len() / 2;
        let right = idx.split_off(mid);
        clusters[ci] = idx;
        clusters.push(right);
    }

    // Средний цвет каждого кластера.
    let mut result: Vec<PaletteColor> = clusters
        .iter()
        .filter(|c| !c.is_empty())
        .map(|cluster| {
            let mut sum = [0f64; 3];
            for &i in cluster {
                sum[0] += pixels[i].r as f64;
                sum[1] += pixels[i].g as f64;
                sum[2] += pixels[i].b as f64;
            }
            let n = cluster.len() as f64;
            let (r, g, b) = (
                (sum[0] / n).round() as u8,
                (sum[1] / n).round() as u8,
                (sum[2] / n).round() as u8,
            );
            PaletteColor {
                hex: format!("#{r:02X}{g:02X}{b:02X}"),
                r,
                g,
                b,
                share: cluster.len() as f32 / total,
            }
        })
        .collect();
    result.sort_by(|a, b| b.share.partial_cmp(&a.share).unwrap());
    result
}

fn channel(p: &Pixel, ch: usize) -> f32 {
    match ch {
        0 => p.r,
        1 => p.g,
        _ => p.b,
    }
}

fn downscale_for_palette(img: &image::DynamicImage, max_side: u32) -> image::DynamicImage {
    let (w, h) = img.dimensions();
    let longest = w.max(h);
    if longest <= max_side {
        return img.clone();
    }
    let scale = max_side as f64 / longest as f64;
    let nw = ((w as f64 * scale).round() as u32).max(1);
    let nh = ((h as f64 * scale).round() as u32).max(1);
    img.resize_exact(nw, nh, image::imageops::FilterType::Triangle)
}

/// Доминирующий цвет (для UI-подсказок). Дешёле, чем полная палитра.
pub fn dominant_color(img: &image::DynamicImage) -> Option<Rgba<u8>> {
    let palette = extract_palette(img, 3);
    palette.first().map(|c| Rgba([c.r, c.g, c.b, 255]))
}

/// Тепловая карта «сколько каких оттенков» — используется в File Compare.
pub fn color_histogram(img: &image::DynamicImage, bins: usize) -> HashMap<String, usize> {
    let bins = bins.clamp(2, 64);
    let rgba = img.to_rgba8();
    let mut map = HashMap::new();
    for px in rgba.pixels() {
        if px[3] < 16 {
            continue;
        }
        let step = (256 / bins) as u8;
        let key = format!(
            "{:02X}{:02X}{:02X}",
            px[0] / step * step,
            px[1] / step * step,
            px[2] / step * step,
        );
        *map.entry(key).or_insert(0) += 1;
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    #[test]
    fn two_color_image_gives_two_dominants() {
        let mut img = ImageBuffer::new(40, 40);
        for (x, _, px) in img.enumerate_pixels_mut() {
            *px = if x < 20 { Rgba([255, 0, 0, 255]) } else { Rgba([0, 0, 255, 255]) };
        }
        let img = image::DynamicImage::ImageRgba8(img);
        let palette = extract_palette(&img, 4);
        assert!(palette.len() >= 2 && palette.len() <= 4);
        let reds = palette.iter().any(|c| c.r > 200 && c.b < 60);
        let blues = palette.iter().any(|c| c.b > 200 && c.r < 60);
        assert!(reds, "нет красного в {palette:?}");
        assert!(blues, "нет синего в {palette:?}");
        // Доли примерно по 50%.
        assert!((palette[0].share - 0.5).abs() < 0.1);
        assert!(palette.iter().all(|c| c.hex.starts_with('#') && c.hex.len() == 7));
    }

    #[test]
    fn single_color_image() {
        let mut img = ImageBuffer::new(10, 10);
        for (_, _, px) in img.enumerate_pixels_mut() {
            *px = Rgba([12, 34, 56, 255]);
        }
        let img = image::DynamicImage::ImageRgba8(img);
        let palette = extract_palette(&img, 5);
        assert!(!palette.is_empty());
        assert_eq!(palette[0].hex, "#0C2238");
        assert!(palette[0].share > 0.9, "share = {}", palette[0].share);
    }

    #[test]
    fn dominant_works() {
        let mut img = ImageBuffer::new(20, 20);
        for (_, _, px) in img.enumerate_pixels_mut() {
            *px = Rgba([0, 200, 0, 255]);
        }
        let img = image::DynamicImage::ImageRgba8(img);
        let dom = dominant_color(&img).unwrap();
        assert!(dom[1] > 180);
    }
}

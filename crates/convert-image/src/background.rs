//! Локальное удаление однотонного фона.
//!
//! Алгоритм: flood fill от краёв изображения. Все пиксели, чей цвет близок
//! к цвету фона (определяется по кромке) и которые достижимы от края,
//! становятся прозрачными. Работает для фото на белом/однотонном фоне —
//! именно такой случай описан в ТЗ. Сложные фоны (волосы, градиенты)
//! честно требуют ML-модуля, который в планах как опция.

use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Параметры удаления фона.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundRemovalParams {
    /// Допуск цвета фона: 0 (точное совпадение) ..= 128.
    pub tolerance: u8,
    /// Смягчение краёв: расширяет прозрачность внутрь объекта на N px.
    pub feather: u32,
}

impl Default for BackgroundRemovalParams {
    fn default() -> Self {
        Self { tolerance: 32, feather: 0 }
    }
}

/// Убирает однотонный фон (заливка от краёв).
pub fn remove_background_solid(
    img: &DynamicImage,
    params: &BackgroundRemovalParams,
) -> DynamicImage {
    let src = img.to_rgba8();
    let (w, h) = src.dimensions();
    if w == 0 || h == 0 {
        return img.clone();
    }

    // 1. Цвет фона — медиана пикселей кромки (2px).
    let mut edge_pixels: Vec<[u8; 3]> = Vec::new();
    for x in 0..w {
        for y in [0, 1, h.saturating_sub(2), h.saturating_sub(1)] {
            if (y as usize) < h as usize {
                let p = src.get_pixel(x, y);
                edge_pixels.push([p[0], p[1], p[2]]);
            }
        }
    }
    for y in 0..h {
        for x in [0, 1, w.saturating_sub(2), w.saturating_sub(1)] {
            if (x as usize) < w as usize {
                let p = src.get_pixel(x, y);
                edge_pixels.push([p[0], p[1], p[2]]);
            }
        }
    }
    let bg = median_color(&edge_pixels);

    // 2. BFS от всех краевых пикселей, близких к фону.
    let tolerance = params.tolerance as i32 * 3; // суммарный допуск по каналам
    let mut visited = vec![false; (w as usize) * (h as usize)];
    let mut transparent = vec![false; (w as usize) * (h as usize)];
    let mut queue: VecDeque<(u32, u32)> = VecDeque::new();

    let close_to_bg = |p: &Rgba<u8>| -> bool {
        (p[0] as i32 - bg[0] as i32).abs()
            + (p[1] as i32 - bg[1] as i32).abs()
            + (p[2] as i32 - bg[2] as i32).abs()
            <= tolerance
    };

    for x in 0..w {
        for &y in &[0, h - 1] {
            if close_to_bg(&src.get_pixel(x, y)) {
                queue.push_back((x, y));
            }
        }
    }
    for y in 0..h {
        for &x in &[0, w - 1] {
            if close_to_bg(&src.get_pixel(x, y)) {
                queue.push_back((x, y));
            }
        }
    }

    while let Some((x, y)) = queue.pop_front() {
        let idx = (y as usize) * (w as usize) + (x as usize);
        if visited[idx] {
            continue;
        }
        visited[idx] = true;
        if !close_to_bg(&src.get_pixel(x, y)) {
            continue;
        }
        transparent[idx] = true;
        // Соседи 4-связности.
        if x > 0 {
            queue.push_back((x - 1, y));
        }
        if x + 1 < w {
            queue.push_back((x + 1, y));
        }
        if y > 0 {
            queue.push_back((x, y - 1));
        }
        if y + 1 < h {
            queue.push_back((x, y + 1));
        }
    }

    // 3. Результат + опциональное смягчение краёв.
    let mut out = ImageBuffer::new(w, h);
    for (x, y, px) in src.enumerate_pixels() {
        let idx = (y as usize) * (w as usize) + (x as usize);
        let mut p = *px;
        if transparent[idx] {
            p[3] = 0;
        }
        out.put_pixel(x, y, p);
    }
    let mut result = DynamicImage::ImageRgba8(out);

    if params.feather > 0 {
        result = feather_edges(&result, params.feather, &transparent, w, h);
    }
    result
}

fn median_color(pixels: &[[u8; 3]]) -> [u8; 3] {
    let mut channels: Vec<Vec<u8>> = vec![Vec::new(), Vec::new(), Vec::new()];
    for p in pixels {
        channels[0].push(p[0]);
        channels[1].push(p[1]);
        channels[2].push(p[2]);
    }
    let mut med = [0u8; 3];
    for (i, ch) in channels.iter_mut().enumerate() {
        if ch.is_empty() {
            med[i] = 0;
            continue;
        }
        ch.sort_unstable();
        med[i] = ch[ch.len() / 2];
    }
    med
}

/// Смягчение: приглушает альфу на границе «прозрачное ↔ непрозрачное».
fn feather_edges(
    img: &DynamicImage,
    radius: u32,
    transparent: &[bool],
    w: u32,
    h: u32,
) -> DynamicImage {
    let src = img.to_rgba8();
    let mut out = ImageBuffer::new(w, h);
    for (x, y, px) in src.enumerate_pixels() {
        let idx = (y as usize) * (w as usize) + (x as usize);
        let mut p = *px;
        if !transparent[idx] {
            // Близко ли к границе объекта?
            let near = (x.saturating_sub(radius)..=(x + radius).min(w - 1))
                .any(|nx| {
                    (y.saturating_sub(radius)..=(y + radius).min(h - 1)).any(|ny| {
                        transparent[(ny as usize) * (w as usize) + (nx as usize)]
                    })
                });
            if near {
                p[3] = (p[3] as f32 * 0.55) as u8;
            }
        }
        out.put_pixel(x, y, p);
    }
    DynamicImage::ImageRgba8(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle_on_white() -> DynamicImage {
        let mut img = ImageBuffer::new(50, 50);
        for (x, y, px) in img.enumerate_pixels_mut() {
            let dx = x as i32 - 25;
            let dy = y as i32 - 25;
            let inside = dx * dx + dy * dy <= 15 * 15;
            *px = if inside { Rgba([200, 30, 30, 255]) } else { Rgba([255, 255, 255, 255]) };
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn removes_white_around_circle() {
        let img = circle_on_white();
        let out = remove_background_solid(&img, &BackgroundRemovalParams::default());
        let rgba = out.to_rgba8();
        // Центр круга остаётся непрозрачным.
        assert_eq!(rgba.get_pixel(25, 25)[3], 255);
        // Углы прозрачны.
        assert_eq!(rgba.get_pixel(2, 2)[3], 0);
        assert_eq!(rgba.get_pixel(47, 47)[3], 0);
        assert_eq!(rgba.get_pixel(2, 47)[3], 0);
    }

    #[test]
    fn tolerance_zero_only_exact() {
        // Фон 250, объект 250 — один цвет: всё станет прозрачным.
        let mut img = ImageBuffer::new(10, 10);
        for (_, _, px) in img.enumerate_pixels_mut() {
            *px = Rgba([250, 250, 250, 255]);
        }
        let img = DynamicImage::ImageRgba8(img);
        let out = remove_background_solid(&img, &BackgroundRemovalParams { tolerance: 10, feather: 0 });
        assert_eq!(out.to_rgba8().get_pixel(5, 5)[3], 0);
    }

    #[test]
    fn colored_background_also_works() {
        let mut img = ImageBuffer::new(30, 30);
        for (x, y, px) in img.enumerate_pixels_mut() {
            let inside = x >= 5 && x < 25 && y >= 5 && y < 25;
            *px = if inside { Rgba([0, 0, 0, 255]) } else { Rgba([10, 200, 30, 255]) };
        }
        let img = DynamicImage::ImageRgba8(img);
        let out = remove_background_solid(&img, &BackgroundRemovalParams::default());
        let rgba = out.to_rgba8();
        assert_eq!(rgba.get_pixel(15, 15)[3], 255);
        assert_eq!(rgba.get_pixel(1, 1)[3], 0);
        assert_eq!(rgba.get_pixel(28, 28)[3], 0);
    }
}

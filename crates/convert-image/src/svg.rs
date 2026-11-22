//! Растеризация SVG через resvg (полностью локально).

use convert_core::error::{ConvertError, ConvertResult};
use image::DynamicImage;
use std::path::Path;

/// Собственный размер SVG (width/height корня) в пикселях, если задан.
pub fn svg_natural_size(path: &Path) -> ConvertResult<Option<(u32, u32)>> {
    let data = std::fs::read(path)?;
    svg_natural_size_from_data(&data)
}

/// Размер из данных SVG.
pub fn svg_natural_size_from_data(data: &[u8]) -> ConvertResult<Option<(u32, u32)>> {
    let tree = usvg_tree(data)?;
    let size = tree.size();
    if size.width() > 0.0 && size.height() > 0.0 {
        Ok(Some((size.width().round() as u32, size.height().round() as u32)))
    } else {
        Ok(None)
    }
}

/// Растеризует SVG в PNG-изображение с заданной шириной (высота — по
/// пропорциям). Если ширина не задана, используется естественный размер
/// или 1024 px по умолчанию.
pub fn rasterize_svg(path: &Path, target_width: Option<u32>) -> ConvertResult<DynamicImage> {
    let data = std::fs::read(path)?;
    let tree = usvg_tree(&data)?;

    let natural = tree.size();
    let width = target_width
        .or_else(|| {
            if natural.width() > 0.0 {
                Some(natural.width().round() as u32)
            } else {
                None
            }
        })
        .unwrap_or(1024)
        .clamp(1, 16384);
    let height = if natural.height() > 0.0 && natural.width() > 0.0 {
        ((natural.height() / natural.width()) * width as f32).round().max(1.0) as u32
    } else {
        width
    };

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| ConvertError::Processing("не удалось создать холст".into()))?;
    let transform = tiny_skia::Transform::from_scale(
        width as f32 / natural.width().max(1.0) as f32,
        height as f32 / natural.height().max(1.0) as f32,
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());    let rgba = image::RgbaImage::from_raw(width, height, pixmap.take())
        .ok_or_else(|| ConvertError::Processing("ошибка растеризации".into()))?;
    Ok(DynamicImage::ImageRgba8(rgba))
}

fn usvg_tree(data: &[u8]) -> ConvertResult<usvg::Tree> {
    let mut opts = usvg::Options::default();
    // Без загрузки системных шрифтов <text> в SVG не рендерится.
    opts.fontdb_mut().load_system_fonts();
    usvg::Tree::from_data(data, &opts)
        .map_err(|e| ConvertError::Processing(format!("SVG: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="80">
      <rect x="0" y="0" width="120" height="80" fill="#3366aa"/>
      <circle cx="60" cy="40" r="30" fill="#ffcc00"/>
    </svg>"##;

    #[test]
    fn natural_size_parsed() {
        let size = svg_natural_size_from_data(SVG.as_bytes()).unwrap();
        assert_eq!(size, Some((120, 80)));
    }

    #[test]
    fn rasterize_to_png() {
        let dir = std::env::temp_dir().join("convert_image_svg");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.svg");
        std::fs::write(&path, SVG).unwrap();

        let img = rasterize_svg(&path, Some(240)).unwrap();
        assert_eq!(img.dimensions(), (240, 160));
        // Проверяем цвет фона и круга.
        let rgba = img.to_rgba8();
        let bg = rgba.get_pixel(2, 2);
        let circle = rgba.get_pixel(120, 80);        assert_eq!(bg, &image::Rgba([0x33, 0x66, 0xaa, 0xff]));
        assert_eq!(circle, &image::Rgba([0xff, 0xcc, 0x00, 0xff]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_svg_is_error() {
        assert!(usvg_tree(b"not an svg at all").is_err());
    }
}

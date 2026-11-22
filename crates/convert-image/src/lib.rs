//! convert-image — движок Image Studio и Quality Enhancer.
//!
//! Полностью локальная обработка изображений на чистом Rust (крейт `image`
//! + `resvg` для SVG). Никаких внешних процессов и облака.
//!
//! Модули:
//! - [`job`] — главная операция: загрузка → кадрирование → масштаб →
//!   поворот → цветокоррекция → кодирование;
//! - [`ops`] — операции над пикселями (яркость, контраст, шум и т.д.);
//! - [`encode`] — кодирование в целевой формат с настройками качества;
//! - [`palette`] — извлечение цветовой палитры (медианный срез);
//! - [`background`] — удаление однотонного фона (заливка от краёв);
//! - [`svg`] — растеризация SVG;
//! - [`metadata`] — чтение/удаление/правка EXIF, DPI, PNG-метаданные.
//!
//! Ограничение честно: HEIC и AVIF не декодируются этим движком —
//! их обрабатывает FFmpeg-движок (convert-media), конвертер сам
//! выберет нужный маршрут.

pub mod background;
pub mod encode;
pub mod job;
pub mod metadata;
pub mod ops;
pub mod palette;
pub mod svg;

pub use background::{remove_background_solid, BackgroundRemovalParams};
pub use encode::{encode_to_file, encode_to_writer, estimate_encoded_size, EncodeParams, ImageOutputFormat};
pub use job::{image_source_info, load_image, run_job, ImageJob, ImageJobResult, ImageSourceInfo};
pub use metadata::{
    exif_edit, jpeg_set_dpi, png_read_dpi, png_set_dpi, privacy_sensitive_fields,
    read_dpi, read_image_metadata, strip_image_metadata, ExifEdit, ImageMetadata, MetadataField,
};
pub use ops::{
    adjust_brightness, adjust_gamma, adjust_saturation, apply_adjustments, auto_contrast,
    denoise_median, sharpen, ColorAdjustments, CropParams, ResizeMode, ResizeParams, RotationOp,
};
pub use palette::{color_histogram, dominant_color, extract_palette, PaletteColor};
pub use svg::{rasterize_svg, svg_natural_size, svg_natural_size_from_data};

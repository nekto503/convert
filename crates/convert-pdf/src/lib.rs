//! convert-pdf — движок PDF Studio.
//!
//! Чистый Rust (lopdf) для слияния, разделения, поворота страниц,
//! метаданных и сборки документов; рендер страниц в изображения — через
//! внешний pdftoppm (poppler) или рендер-фолбэк (см. render.rs).
//!
//! Возможности PDF Studio (ТЗ 4.6):
//! merge, split, extract/delete/reorder/rotate pages, page numbering,
//! watermark, metadata, compress (пересборка), text extraction,
//! PDF → изображения, изображения → PDF, текст → DOCX, поиск по тексту.

pub mod docx;
pub mod images;
pub mod meta;
pub mod ops;
pub mod pages;
pub mod render;
pub mod text;
pub mod watermark;

pub use docx::docx_from_text;
pub use images::{images_to_pdf, pdf_page_count};
pub use meta::{read_pdf_metadata, set_pdf_metadata, PdfMetadata};
pub use ops::{compress_pdf, extract_pdf_images};
pub use pages::{
    delete_pages, extract_pages, merge_pdfs, reorder_pages, rotate_page, rotate_all_pages,
    split_pdf, PageSelection,
};
pub use render::render_pages_to_images;
pub use text::{extract_text, page_text};
pub use watermark::{add_page_numbers, add_text_watermark, NumberPosition, WatermarkOptions};

// Вспомогательные функции для OCR-конвейера convert-media.
pub use render::render_pages_to_images as render_for_ocr;

mod searchable;
pub use searchable::build_searchable_pdf;

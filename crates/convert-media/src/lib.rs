//! convert-media — движки Video Studio, Audio Studio, Document Studio
//! и OCR Studio.
//!
//! Обёртки над внешними локальными движками:
//! - **FFmpeg/FFprobe** — видео и аудио (конвертация, кодеки, обрезка,
//!   сжатие, громкость, нормализация, извлечение кадров и аудио);
//! - **LibreOffice (soffice)** — офисные документы (DOCX/ODT/RTF/XLSX/
//!   PPTX → PDF и обратно, извлечение текста);
//! - **Tesseract OCR** — распознавание текста (rus+eng, экспорт в
//!   TXT/DOCX/PDF с текстовым слоем через hOCR).
//!
//! Все движки вызываются как процессы с таймаутами, результат парсится.
//! В песочнице доступны реальные ffmpeg/soffice/tesseract, поэтому тесты
//! выполняют настоящие конвертации.
//!
//! Модули:
//! - [`process`] — надёжный запуск внешних процессов с таймаутом;
//! - [`probe`] — разбор JSON от ffprobe (кодеки, разрешение, битрейт);
//! - [`video`] — операции с видео;
//! - [`audio`] — операции с аудио;
//! - [`office`] — конвертация документов через LibreOffice;
//! - [`ocr`] — распознавание текста через Tesseract.

pub mod audio;
pub mod office;
pub mod ocr;
pub mod process;
pub mod probe;
pub mod video;

pub use audio::{normalize_audio, run_audio_job, AudioJob, AudioOutputFormat};
pub use office::{docx_to_txt, office_convert, office_text, LibreOfficeFormat};
pub use ocr::{
    ocr_image, ocr_pdf_to_searchable, ocr_to_docx, OcrLanguage, OcrOutput,
};
pub use process::{engines_status, run_process, run_process_with_timeout, EngineLocation, EngineStatus};
pub use probe::{probe_media, AudioStream, MediaInfo, VideoStream};
pub use video::{
    concat_videos, extract_audio, extract_frames, run_video_job, screenshot, VideoJob,
    VideoOutputFormat,
};

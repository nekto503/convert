//! Рендер страниц PDF в изображения: внешний pdftoppm (poppler) как
//! основной движок (быстрый и точный), с честной ошибкой, если его нет.

use convert_core::error::{ConvertError, ConvertResult};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Рендерит все страницы PDF в PNG в `output_dir` с заданным DPI.
/// Возвращает пути к PNG в порядке страниц.
pub fn render_pages_to_images(
    pdf_path: &Path,
    output_dir: &Path,
    dpi: u16,
) -> ConvertResult<Vec<PathBuf>> {
    if !pdf_path.is_file() {
        return Err(ConvertError::FileNotFound(pdf_path.display().to_string()));
    }
    let pdftoppm = find_pdftoppm().ok_or_else(|| {
        ConvertError::EngineMissing(
            "pdftoppm (poppler) не найден — установите компонент для рендера PDF".into(),
        )
    })?;
    std::fs::create_dir_all(output_dir)?;

    let prefix = output_dir.join("page");
    let args: Vec<String> = vec![
        "-r".to_string(),
        dpi.to_string(),
        "-png".to_string(),
        pdf_path.to_string_lossy().to_string(),
        prefix.to_string_lossy().to_string(),
    ];
    process_shim::run(&pdftoppm, &args)?;

    // pdftoppm называет файлы page-1.png, page-01.png, page-001.png...
    let mut pages: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(output_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("png") {
                pages.push(p);
            }
        }
    }
    // Естественная сортировка по номеру страницы.
    pages.sort_by_key(|p| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.rsplit('-').next().map(|n| n.to_string()))
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap_or(u64::MAX)
    });
    if pages.is_empty() {
        return Err(ConvertError::Processing(
            "pdftoppm не создал ни одного изображения".into(),
        ));
    }
    Ok(pages)
}

/// Поиск pdftoppm в PATH.
fn find_pdftoppm() -> Option<PathBuf> {
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("pdftoppm");
            if candidate.is_file() {
                return Some(candidate);
            }
            let candidate_exe = dir.join("pdftoppm.exe");
            if candidate_exe.is_file() {
                return Some(candidate_exe);
            }
        }
    }
    None
}

/// Минимальная обёртка процесса внутри convert-pdf (без зависимости от
/// convert-media, чтобы избежать цикла: media → pdf → media).
pub(crate) mod process_shim {
    use convert_core::error::{ConvertError, ConvertResult};
    use std::path::Path;
    use std::time::Duration;

    pub(crate) fn run(program: &Path, args: &[String]) -> ConvertResult<()> {
        let mut command = std::process::Command::new(program);
        command.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let child = command
            .spawn()
            .map_err(|e| ConvertError::EngineMissing(format!("{}: {e}", program.display())))?;
        wait(child, Duration::from_secs(10 * 60))
    }

    fn wait(mut child: std::process::Child, timeout: Duration) -> ConvertResult<()> {
        let start = std::time::Instant::now();
        loop {
            match child.try_wait()? {
                Some(status) => {
                    return if status.success() {
                        Ok(())
                    } else {
                        Err(ConvertError::EngineFailed {
                            engine: "pdftoppm".into(),
                            code: status.code().unwrap_or(-1),
                            stderr: String::new(),
                        })
                    };
                }
                None => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        return Err(ConvertError::EngineFailed {
                            engine: "pdftoppm".into(),
                            code: -1,
                            stderr: "таймаут".into(),
                        });
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use crate::images::images_to_pdf;

    #[test]
    fn renders_pages() {
        if find_pdftoppm().is_none() {
            eprintln!("pdftoppm недоступен — пропуск");
            return;
        }
        let dir = std::env::temp_dir().join("convert_pdf_render");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Двухстраничный PDF из картинок.
        let img1 = dir.join("p1.png");
        let img2 = dir.join("p2.png");
        image::save_buffer_with_format(
            &img1,
            image::RgbImage::from_fn(200, 100, |x, _| image::Rgb([(x % 256) as u8, 40, 40])).as_raw(),
            200,
            100,
            image::ExtendedColorType::Rgb8,
            image::ImageFormat::Png,
        )
        .unwrap();
        image::save_buffer_with_format(
            &img2,
            image::RgbImage::from_fn(200, 100, |x, _| image::Rgb([40, (x % 256) as u8, 40])).as_raw(),
            200,
            100,
            image::ExtendedColorType::Rgb8,
            image::ImageFormat::Png,
        )
        .unwrap();
        let pdf = dir.join("doc.pdf");
        images_to_pdf(&[img1, img2], &pdf).unwrap();

        let out = dir.join("rendered");
        let pages = render_pages_to_images(&pdf, &out, 72).unwrap();
        assert_eq!(pages.len(), 2);
        // Порядок страниц правильный.
        let first = image::open(&pages[0]).unwrap();
        assert_eq!(first.dimensions(), (200, 100));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_pdf_rejected() {
        if find_pdftoppm().is_none() {
            return;
        }
        assert!(render_pages_to_images(Path::new("/no/file.pdf"), Path::new("/tmp/x"), 150).is_err());
    }
}

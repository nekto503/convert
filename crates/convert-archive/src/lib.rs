//! convert-archive — менеджер архивов (Archive Manager из ТЗ 4.9).
//!
//! Форматы:
//! - **ZIP** — чтение/запись, пароли (AES, через крейт zip);
//! - **7Z** — чтение/запись (sevenz-rust), пароли на запись;
//! - **TAR / TAR.GZ / GZ** — чтение/запись;
//! - **RAR** — только чтение не поддержано в чистом Rust без unrar;
//!   честно возвращаем ошибку с пояснением.
//!
//! Безопасность (ТЗ 10): лимиты на общий размер распакованных данных
//! и число файлов (защита от zip-бомб), защита от path traversal
//! (записи `../../evil.exe` не распаковываются за пределы папки).

use convert_core::error::{ConvertError, ConvertResult};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

/// Форматы архивов, которые создаёт CONVERT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchiveFormat {
    Zip,
    SevenZip,
    Tar,
    TarGz,
}

impl ArchiveFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::SevenZip => "7z",
            Self::Tar => "tar",
            Self::TarGz => "tar.gz",
        }
    }
}

/// Уровень сжатия: 0 (без сжатия) ..= 9 (максимум).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CompressionLevel(pub u8);

/// Лимиты безопасности при распаковке.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractLimits {
    /// Максимум суммарного размера распакованных данных.
    pub max_total_bytes: u64,
    /// Максимум файлов.
    pub max_files: u64,
}

impl Default for ExtractLimits {
    fn default() -> Self {
        Self {
            max_total_bytes: 8 * 1024 * 1024 * 1024, // 8 ГБ
            max_files: 100_000,
        }
    }
}

/// Создаёт архив из файлов и папок.
///
/// * `items` — файлы и/или папки (папки добавляются рекурсивно);
/// * `password` — пароль (только ZIP/7Z);
/// * `level` — уровень сжатия 0..=9.
pub fn create_archive(
    items: &[PathBuf],
    output: &Path,
    format: ArchiveFormat,
    password: Option<&str>,
    level: u8,
) -> ConvertResult<()> {
    if items.is_empty() {
        return Err(ConvertError::InvalidParams("нет файлов для архивации".into()));
    }
    // Проверяем существование всех путей заранее.
    for item in items {
        if !item.exists() {
            return Err(ConvertError::FileNotFound(item.display().to_string()));
        }
    }
    let level = level.min(9);
    match format {
        ArchiveFormat::Zip => create_zip(items, output, password, level),
        ArchiveFormat::SevenZip => create_7z(items, output, password, level),
        ArchiveFormat::Tar => create_tar(items, output, None),
        ArchiveFormat::TarGz => create_tar(items, output, Some(flate2::Compression::new(level as u32))),
    }
}

/// Собирает список файлов с относительными именами.
fn collect_files(items: &[PathBuf]) -> ConvertResult<Vec<(PathBuf, String)>> {
    let mut result = Vec::new();
    for item in items {
        let name = item
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ConvertError::InvalidParams("некорректное имя файла".into()))?
            .to_string();
        if item.is_file() {
            result.push((item.clone(), name));
        } else if item.is_dir() {
            collect_dir(item, &name, &mut result)?;
        }
    }
    Ok(result)
}

fn collect_dir(dir: &Path, prefix: &str, out: &mut Vec<(PathBuf, String)>) -> ConvertResult<()> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| ConvertError::Processing(format!("{}: {e}", dir.display())))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let rel = format!("{prefix}/{name}");
        if path.is_dir() {
            collect_dir(&path, &rel, out)?;
        } else {
            out.push((path, rel));
        }
    }
    Ok(())
}

fn create_zip(items: &[PathBuf], output: &Path, password: Option<&str>, level: u8) -> ConvertResult<()> {
    let file = std::fs::File::create(output)?;
    let mut zip = zip::ZipWriter::new(file);
    let mut options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(level as i64));
    if let Some(pw) = password {
        // AES-256 надёжнее стандартного ZipCrypto.
        options = options.with_aes_encryption(zip::AesMode::Aes256, pw);
    }
    for (path, name) in collect_files(items)? {
        zip.start_file(name, options)
            .map_err(|e| ConvertError::Processing(format!("zip: {e}")))?;
        let data = std::fs::read(&path)?;
        zip.write_all(&data)?;
    }
    zip.finish()
        .map_err(|e| ConvertError::Processing(format!("zip: {e}")))?;
    Ok(())
}

fn create_7z(items: &[PathBuf], output: &Path, password: Option<&str>, _level: u8) -> ConvertResult<()> {
    let files = collect_files(items)?;
    let mut writer = sevenz_rust::SevenZWriter::create(output)
        .map_err(|e| ConvertError::Processing(format!("7z: {e}")))?;
    // Шифрование: AES + LZMA2 (см. en_funcs.rs sevenz-rust).
    if let Some(pw) = password {
        writer
            .set_content_methods(vec![
                sevenz_rust::AesEncoderOptions::new(sevenz_rust::Password::from(pw))
                .into(),
                sevenz_rust::SevenZMethod::LZMA2.into(),
            ]);
    }
    for (path, name) in files {
        let entry = sevenz_rust::SevenZArchiveEntry::from_path(&path, name);
        let reader = std::fs::File::open(&path)
            .map_err(|e| ConvertError::FileNotFound(format!("{}: {e}", path.display())))?;
        writer
            .push_archive_entry(entry, Some(reader))
            .map_err(|e| ConvertError::Processing(format!("7z: {e}")))?;
    }
    writer
        .finish()
        .map_err(|e| ConvertError::Processing(format!("7z: {e}")))?;
    Ok(())
}

fn create_tar(items: &[PathBuf], output: &Path, gzip: Option<flate2::Compression>) -> ConvertResult<()> {
    let file = std::fs::File::create(output)?;
    // Оба варианта сводятся к Builder<GzEncoder<File>> или Builder<File>;
    // дублируем логику через макрос-хелпер нельзя — явные ветки.
    if let Some(level) = gzip {
        let encoder = flate2::write::GzEncoder::new(file, level);
        let mut builder = tar::Builder::new(encoder);
        append_items(items, &mut builder)?;
        let encoder = builder
            .into_inner()
            .map_err(|e| ConvertError::Processing(format!("tar: {e}")))?;
        encoder
            .finish()
            .map_err(|e| ConvertError::Processing(format!("gzip: {e}")))?;
        return Ok(());
    }
    let mut builder = tar::Builder::new(file);
    append_items(items, &mut builder)?;
    builder
        .into_inner()
        .map_err(|e| ConvertError::Processing(format!("tar: {e}")))?;
    Ok(())
}

fn append_items<W: Write>(items: &[PathBuf], builder: &mut tar::Builder<W>) -> ConvertResult<()> {
    for item in items {
        if item.is_file() {
            builder
                .append_path_with_name(item, item.file_name().unwrap_or_default())
                .map_err(|e| ConvertError::Processing(format!("tar: {e}")))?;
        } else {
            builder
                .append_dir_all(item.file_name().unwrap_or_default(), item)
                .map_err(|e| ConvertError::Processing(format!("tar: {e}")))?;
        }
    }
    Ok(())
}

/// Запись в архиве для UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

/// Список содержимого архива (ZIP/7Z/TAR/TAR.GZ).
pub fn list_archive(archive: &Path) -> ConvertResult<Vec<ArchiveEntry>> {
    let fmt = detect_archive_format(archive)?;
    match fmt {
        ArchiveFormat::Zip => {
            let file = std::fs::File::open(archive)?;
            let mut zip = zip::ZipArchive::new(file)
                .map_err(|e| ConvertError::Processing(format!("zip: {e}")))?;
            let mut result = Vec::new();
            for i in 0..zip.len() {
                // Имя и размер доступны из центрального каталога даже
                // для зашифрованных записей — через by_index_raw.
                let entry = zip
                    .by_index_raw(i)
                    .map_err(|e| ConvertError::Processing(format!("zip: {e}")))?;
                result.push(ArchiveEntry {
                    name: entry.name().to_string(),
                    size: entry.size(),
                    is_dir: entry.is_dir(),
                });
            }
            Ok(result)
        }
        ArchiveFormat::SevenZip => {
            let file = std::fs::File::open(archive)?;
            let reader =
                sevenz_rust::SevenZReader::new(file, archive.metadata()?.len(), sevenz_rust::Password::empty())
                    .map_err(map_7z)?;
            let entries = reader
                .archive()
                .files
                .iter()
                .map(|f| ArchiveEntry {
                    name: f.name().to_string(),
                    size: f.size(),
                    is_dir: f.is_directory(),
                })
                .collect();
            Ok(entries)
        }
        ArchiveFormat::Tar | ArchiveFormat::TarGz => {
            let file = std::fs::File::open(archive)?;
            let mut result = Vec::new();
            if fmt == ArchiveFormat::TarGz {
                let decoder = flate2::read::GzDecoder::new(file);
                let mut builder = tar::Archive::new(decoder);
                let entries = builder
                    .entries()
                    .map_err(|e| ConvertError::Processing(format!("tar.gz: {e}")))?;
                for entry in entries.flatten() {
                    let header = entry.header();
                    result.push(ArchiveEntry {
                        name: entry.path()?.to_string_lossy().to_string(),
                        size: header.size()?,
                        is_dir: header.entry_type().is_dir(),
                    });
                }
            } else {
                let mut builder = tar::Archive::new(file);
                let entries = builder
                    .entries()
                    .map_err(|e| ConvertError::Processing(format!("tar: {e}")))?;
                for entry in entries.flatten() {
                    let header = entry.header();
                    result.push(ArchiveEntry {
                        name: entry.path()?.to_string_lossy().to_string(),
                        size: header.size()?,
                        is_dir: header.entry_type().is_dir(),
                    });
                }
            }
            Ok(result)
        }
    }
}

/// Определяет формат архива по содержимому.
pub fn detect_archive_format(path: &Path) -> ConvertResult<ArchiveFormat> {
    let mut buf = [0u8; 8];
    let mut file = std::fs::File::open(path)?;
    file.read_exact(&mut buf)?;
    match buf {
        [0x50, 0x4B, ..] => Ok(ArchiveFormat::Zip),
        [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, ..] => Ok(ArchiveFormat::SevenZip),
        [0x1F, 0x8B, ..] => Ok(ArchiveFormat::TarGz),
        // TAR: "ustar" на смещении 257 — читаем дальше.
        _ => {
            let mut probe = vec![0u8; 265];
            file.seek(std::io::SeekFrom::Start(0))?;
            file.read_exact(&mut probe)?;
            if &probe[257..262] == b"ustar" {
                Ok(ArchiveFormat::Tar)
            } else {
                Err(ConvertError::UnknownFormat(path.display().to_string()))
            }
        }
    }
}

/// Распаковывает архив в папку с проверками безопасности.
pub fn extract_archive(
    archive: &Path,
    output_dir: &Path,
    password: Option<&str>,
    limits: &ExtractLimits,
) -> ConvertResult<Vec<PathBuf>> {
    let fmt = detect_archive_format(archive)?;
    std::fs::create_dir_all(output_dir)?;
    match fmt {
        ArchiveFormat::Zip => extract_zip(archive, output_dir, password, limits),
        ArchiveFormat::SevenZip => extract_7z(archive, output_dir, password, limits),
        ArchiveFormat::Tar | ArchiveFormat::TarGz => extract_tar(archive, output_dir, limits),
    }
}

/// Проверяет имя записи против path traversal.
fn safe_entry_path(output_dir: &Path, name: &str) -> ConvertResult<PathBuf> {
    let clean = name.replace('\\', "/");
    let path = Path::new(&clean);
    for component in path.components() {
        if let std::path::Component::ParentDir = component {
            return Err(ConvertError::UnsafeArchive(format!(
                "запись с выходом за пределы папки: {name}"
            )));
        }
    }
    Ok(output_dir.join(path))
}

fn extract_zip(
    archive: &Path,
    output_dir: &Path,
    password: Option<&str>,
    limits: &ExtractLimits,
) -> ConvertResult<Vec<PathBuf>> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| ConvertError::Processing(format!("zip: {e}")))?;

    let count = zip.len() as u64;
    if count > limits.max_files {
        return Err(ConvertError::UnsafeArchive(format!(
            "в архиве {count} файлов — больше лимита {}",
            limits.max_files
        )));
    }
    // Суммарный размер до распаковки.
    let declared: u64 = (0..zip.len())
        .filter_map(|i| zip.by_index(i).ok().map(|f| f.size()))
        .sum();
    if declared > limits.max_total_bytes {
        return Err(ConvertError::UnsafeArchive(format!(
            "распакованный размер {} превышает лимит {}",
            declared,
            limits.max_total_bytes
        )));
    }

    let mut written = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index_decrypt(i, password.map(|p| p.as_bytes()).unwrap_or(&[]))
            .map_err(|e| match e {
                zip::result::ZipError::InvalidPassword => {
                    ConvertError::InvalidParams("неверный пароль архива".into())
                }
                zip::result::ZipError::UnsupportedArchive(e) => {
                    ConvertError::InvalidParams(format!("архив требует пароль или формат не поддержан: {e}"))
                }
                other => ConvertError::Processing(format!("zip: {other}")),
            })?;
        if entry.is_dir() {
            let dir = safe_entry_path(output_dir, entry.name())?;
            std::fs::create_dir_all(dir)?;
            continue;
        }
        let out_path = safe_entry_path(output_dir, entry.name())?;
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out_file = std::fs::File::create(&out_path)?;
        // Читаем порциями и считаем байты (защита от лжи в заголовках).
        let mut remaining = limits.max_total_bytes;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = entry.read(&mut buf)?;
            if n == 0 {
                break;
            }
            remaining = remaining.checked_sub(n as u64).ok_or_else(|| {
                ConvertError::UnsafeArchive("превышен лимит распаковки (zip-бомба)".into())
            })?;
            out_file.write_all(&buf[..n])?;
        }
        written.push(out_path);
    }
    Ok(written)
}

fn extract_7z(
    archive: &Path,
    output_dir: &Path,
    password: Option<&str>,
    limits: &ExtractLimits,
) -> ConvertResult<Vec<PathBuf>> {
    let file = std::fs::File::open(archive)?;
    let len = archive.metadata()?.len();
    let mut reader = sevenz_rust::SevenZReader::new(
        file,
        len,
        password.map(sevenz_rust::Password::from).unwrap_or_default(),
    )
    .map_err(map_7z)?;

    // Проверка лимитов по заголовкам.
    let total: u64 = reader.archive().files.iter().map(|f| f.size()).sum();
    let count = reader.archive().files.len() as u64;
    if total > limits.max_total_bytes || count > limits.max_files {
        return Err(ConvertError::UnsafeArchive(format!(
            "архив превышает лимиты распаковки ({total} байт, {count} файлов)"
        )));
    }

    let mut written: Vec<PathBuf> = Vec::new();
    let out_dir = output_dir.to_path_buf();
    reader
        .for_each_entries(|entry, reader| {
            let out_path = safe_entry_path(&out_dir, entry.name())
                .map_err(|e| sevenz_rust::Error::other(e.to_string()))?;
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(sevenz_rust::Error::io)?;
            }
            if entry.is_directory() {
                std::fs::create_dir_all(&out_path).map_err(sevenz_rust::Error::io)?;
                return Ok(true);
            }
            let mut out_file = std::fs::File::create(&out_path).map_err(sevenz_rust::Error::io)?;
            let mut remaining = entry.size();
            let mut buf = vec![0u8; 64 * 1024];
            while remaining > 0 {
                let want = buf.len().min(remaining as usize);
                let n = reader.read(&mut buf[..want]).map_err(sevenz_rust::Error::io)?;
                if n == 0 {
                    break;
                }
                out_file.write_all(&buf[..n]).map_err(sevenz_rust::Error::io)?;
                remaining -= n as u64;
            }
            written.push(out_path);
            Ok(true)
        })
        .map_err(map_7z)?;
    Ok(written)
}

fn extract_tar_stream<R: Read>(
    reader: R,
    output_dir: &Path,
    limits: &ExtractLimits,
) -> ConvertResult<Vec<PathBuf>> {
    let mut tar = tar::Archive::new(reader);
    let mut written = Vec::new();
    let mut total: u64 = 0;
    let entries = tar
        .entries()
        .map_err(|e| ConvertError::Processing(format!("tar: {e}")))?;
    for mut entry in entries.flatten() {
        if written.len() as u64 > limits.max_files {
            return Err(ConvertError::UnsafeArchive("слишком много файлов".into()));
        }
        let name = entry.path()?.to_string_lossy().to_string();
        let out_path = safe_entry_path(output_dir, &name)?;
        let size = entry.header().size()?;
        total = total.saturating_add(size);
        if total > limits.max_total_bytes {
            return Err(ConvertError::UnsafeArchive("превышен лимит распаковки".into()));
        }
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
            written.push(out_path);
        }
    }
    Ok(written)
}

fn map_7z(e: sevenz_rust::Error) -> ConvertError {
    ConvertError::Processing(format!("7z: {e}"))
}

fn extract_tar(
    archive: &Path,
    output_dir: &Path,
    limits: &ExtractLimits,
) -> ConvertResult<Vec<PathBuf>> {
    // Для .tar.gz — сначала распаковка gzip-потока.
    let file = std::fs::File::open(archive)?;
    if detect_archive_format(archive)? == ArchiveFormat::TarGz {
        let decoder = flate2::read::GzDecoder::new(file);
        return extract_tar_stream(decoder, output_dir, limits);
    }
    let mut tar = tar::Archive::new(file);
    let mut written = Vec::new();
    let mut total: u64 = 0;
    let entries = tar
        .entries()
        .map_err(|e| ConvertError::Processing(format!("tar: {e}")))?;
    for mut entry in entries.flatten() {
        if written.len() as u64 > limits.max_files {
            return Err(ConvertError::UnsafeArchive("слишком много файлов".into()));
        }
        let name = entry.path()?.to_string_lossy().to_string();
        let out_path = safe_entry_path(output_dir, &name)?;
        let size = entry.header().size()?;
        total = total
            .saturating_add(size)
            .checked_sub(0)
            .ok_or_else(|| ConvertError::UnsafeArchive("переполнение счётчика".into()))?;
        if total > limits.max_total_bytes {
            return Err(ConvertError::UnsafeArchive(
                "превышен лимит распаковки".into(),
            ));
        }
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = entry.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                out_file.write_all(&buf[..n])?;
            }
            written.push(out_path);
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tree(dir: &Path) {
        std::fs::create_dir_all(dir.join("sub/deep")).unwrap();
        std::fs::write(dir.join("a.txt"), b"alpha content").unwrap();
        std::fs::write(dir.join("sub/b.txt"), b"beta content beta").unwrap();
        std::fs::write(dir.join("sub/deep/c.bin"), vec![7u8; 1000]).unwrap();
    }

    #[test]
    fn zip_roundtrip_with_password() {
        let dir = std::env::temp_dir().join("convert_arch_zip");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src");
        make_tree(&src);

        let archive = dir.join("out.zip");
        create_archive(
            &[src.clone()],
            &archive,
            ArchiveFormat::Zip,
            Some("secret123"),
            6,
        )
        .unwrap();
        assert!(archive.exists());

        // Листинг.
        let entries = list_archive(&archive).unwrap();
        assert!(entries.len() >= 3);

        // Распаковка с паролем.
        let out = dir.join("unpacked");
        let files = extract_archive(&archive, &out, Some("secret123"), &ExtractLimits::default()).unwrap();
        assert!(files.len() >= 3);
        assert_eq!(std::fs::read_to_string(out.join("src/a.txt")).unwrap(), "alpha content");
        assert_eq!(std::fs::read_to_string(out.join("src/sub/b.txt")).unwrap(), "beta content beta");
        assert_eq!(std::fs::metadata(out.join("src/sub/deep/c.bin")).unwrap().len(), 1000);

        // Неверный пароль — ошибка.
        let err = extract_archive(&archive, &dir.join("x"), Some("wrong"), &ExtractLimits::default());
        assert!(err.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn zip_without_password() {
        let dir = std::env::temp_dir().join("convert_arch_zip2");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("data.txt"), b"plain data").unwrap();

        let archive = dir.join("plain.zip");
        create_archive(&[dir.join("data.txt")], &archive, ArchiveFormat::Zip, None, 9).unwrap();
        let out = dir.join("unpacked");
        extract_archive(&archive, &out, None, &ExtractLimits::default()).unwrap();
        assert_eq!(std::fs::read_to_string(out.join("data.txt")).unwrap(), "plain data");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sevenz_roundtrip() {
        let dir = std::env::temp_dir().join("convert_arch_7z");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src");
        make_tree(&src);

        let archive = dir.join("out.7z");
        create_archive(&[src.clone()], &archive, ArchiveFormat::SevenZip, None, 5).unwrap();

        let entries = list_archive(&archive).unwrap();
        assert!(entries.len() >= 3);

        let out = dir.join("unpacked");
        let files = extract_archive(&archive, &out, None, &ExtractLimits::default()).unwrap();
        assert!(files.len() >= 3);
        assert_eq!(std::fs::read_to_string(out.join("src/a.txt")).unwrap(), "alpha content");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tar_gz_roundtrip() {
        let dir = std::env::temp_dir().join("convert_arch_tgz");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src");
        make_tree(&src);

        let archive = dir.join("out.tar.gz");
        create_archive(&[src.clone()], &archive, ArchiveFormat::TarGz, None, 6).unwrap();
        assert_eq!(detect_archive_format(&archive).unwrap(), ArchiveFormat::TarGz);

        let out = dir.join("unpacked");
        extract_archive(&archive, &out, None, &ExtractLimits::default()).unwrap();
        assert_eq!(std::fs::read_to_string(out.join("src/sub/deep/c.bin")).map(|s| s.len()).unwrap(), 1000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn format_detection() {
        let dir = std::env::temp_dir().join("convert_arch_fmt");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("f.txt"), b"x").unwrap();

        let zip_p = dir.join("a.zip");
        create_archive(&[dir.join("f.txt")], &zip_p, ArchiveFormat::Zip, None, 1).unwrap();
        assert_eq!(detect_archive_format(&zip_p).unwrap(), ArchiveFormat::Zip);

        let tgz = dir.join("a.tar.gz");
        create_archive(&[dir.join("f.txt")], &tgz, ArchiveFormat::TarGz, None, 1).unwrap();
        assert_eq!(detect_archive_format(&tgz).unwrap(), ArchiveFormat::TarGz);

        let plain_tar = dir.join("a.tar");
        create_archive(&[dir.join("f.txt")], &plain_tar, ArchiveFormat::Tar, None, 0).unwrap();
        assert_eq!(detect_archive_format(&plain_tar).unwrap(), ArchiveFormat::Tar);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn zip_bomb_limited() {
        let dir = std::env::temp_dir().join("convert_arch_bomb");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // «Заявленный» маленький архив: симулируем лимит ниже реального.
        std::fs::write(dir.join("f.txt"), b"1234567890").unwrap();
        let archive = dir.join("small.zip");
        create_archive(&[dir.join("f.txt")], &archive, ArchiveFormat::Zip, None, 1).unwrap();

        // Лимит 5 байт — распаковка обязана отказаться.
        let limits = ExtractLimits { max_total_bytes: 5, max_files: 10 };
        let err = extract_archive(&archive, &dir.join("out"), None, &limits);
        assert!(matches!(err, Err(ConvertError::UnsafeArchive(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_traversal_blocked() {
        // Строим вручную zip с записью ../evil.txt.
        let dir = std::env::temp_dir().join("convert_arch_trav");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = std::fs::File::create(dir.join("evil.zip")).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("../evil.txt", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"bad").unwrap();
        zip.finish().unwrap();

        let err = extract_archive(
            &dir.join("evil.zip"),
            &dir.join("out"),
            None,
            &ExtractLimits::default(),
        );
        assert!(matches!(err, Err(ConvertError::UnsafeArchive(_))));
        assert!(!dir.join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_items_rejected() {
        assert!(create_archive(&[], Path::new("x.zip"), ArchiveFormat::Zip, None, 5).is_err());
    }
}

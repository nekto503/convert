//! Надёжный запуск внешних движков (ffmpeg, soffice, tesseract).
//!
//! Особенности:
//! - движок ищется в PATH и в известных местах установки Windows;
//! - у процесса есть таймаут, чтобы зависший ffmpeg не подвесил очередь;
//! - stderr обрезается до разумной длины для показа пользователю.

use convert_core::error::{ConvertError, ConvertResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Где искать движок.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineLocation {
    /// Имя исполняемого файла (ffmpeg / soffice / tesseract).
    pub exe_name: String,
    /// Найденный путь или None.
    pub found_path: Option<PathBuf>,
}

/// Состояние движка в системе.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatus {
    pub engine: String,
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

/// Ищет движок: PATH → известные каталоги Windows.
pub fn find_engine(exe_name: &str) -> Option<PathBuf> {
    // 1. PATH.
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(exe_name);
            if candidate.is_file() {
                return Some(candidate);
            }
            // Windows: пробуем с .exe.
            let candidate_exe = dir.join(format!("{exe_name}.exe"));
            if candidate_exe.is_file() {
                return Some(candidate_exe);
            }
        }
    }
    // 2. Известные места установки (Windows).
    let program_files = [
        "C:\\Program Files",
        "C:\\Program Files (x86)",
    ];
    let subdirs: &[(&str, &[&str])] = &[
        ("ffmpeg", &["ffmpeg\\bin", "ffmpeg", "FFmpeg\\bin"]),
        ("ffprobe", &["ffmpeg\\bin", "ffmpeg", "FFmpeg\\bin"]),
        ("soffice", &["LibreOffice\\program"]),
        ("tesseract", &["Tesseract-OCR", "Tesseract-OCR\\bin"]),
    ];
    if let Some((_, dirs)) = subdirs.iter().find(|(name, _)| *name == exe_name) {
        for pf in program_files {
            for sub in dirs.iter() {
                let candidate = PathBuf::from(pf).join(sub).join(format!("{exe_name}.exe"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Запускает процесс и ждёт завершения. `timeout` защищает от зависаний.
pub fn run_process_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> ConvertResult<std::process::Output> {
    let mut command = Command::new(program);
    command.args(args);
    // Пайпы обязательны: без них stdout наследуется и остаётся пустым.
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());
    // Прячем консольные окна на Windows.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|e| ConvertError::EngineMissing(format!("{program}: {e}")))?;

    // Дренаж пайпов в фоновых потоках: исключает дедлок при большом
    // выводе (прогресс ffmpeg пишет в stderr мегабайты).
    let stdout_handle = child.stdout.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut s, &mut buf).ok();
            buf
        })
    });
    let stderr_handle = child.stderr.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut s, &mut buf).ok();
            buf
        })
    });

    // Ожидание с таймаутом.
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait()? {
            Some(status) => break status,
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ConvertError::EngineFailed {
                        engine: program.to_string(),
                        code: -1,
                        stderr: format!("превышен таймаут {timeout:?}"),
                    });
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };

    let stdout = stdout_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    let stderr = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();

    let output = std::process::Output { status, stdout, stderr };
    if !output.status.success() {
        let tail = String::from_utf8_lossy(&output.stderr);
        let tail = tail.trim();
        // Последние ~600 символов stderr — там обычно причина.
        let tail = if tail.len() > 600 { &tail[tail.len() - 600..] } else { tail };
        return Err(ConvertError::EngineFailed {
            engine: program.to_string(),
            code: output.status.code().unwrap_or(-1),
            stderr: tail.to_string(),
        });
    }
    Ok(output)
}

/// Упрощённый вызов без явного таймаута (по умолчанию 30 минут).
pub fn run_process(program: &str, args: &[&str]) -> ConvertResult<std::process::Output> {
    run_process_with_timeout(program, args, Duration::from_secs(30 * 60))
}

/// Возвращает путь к движку или ошибку с внятным сообщением.
pub fn require_engine(exe_name: &str) -> ConvertResult<PathBuf> {
    find_engine(exe_name).ok_or_else(|| {
        ConvertError::EngineMissing(format!(
            "{exe_name} не найден. Установите компонент или укажите путь в настройках"
        ))
    })
}

/// Собирает статус всех движков (для страницы «Установленные компоненты»).
pub fn engines_status() -> Vec<EngineStatus> {
    let mut result = Vec::new();
    for name in ["ffmpeg", "ffprobe", "soffice", "tesseract"] {
        let status = match find_engine(name) {
            Some(path) => {
                let version = run_process_with_timeout(
                    path.to_str().unwrap_or(name),
                    &["-version"],
                    Duration::from_secs(10),
                )
                .ok()
                .and_then(|out| {
                    String::from_utf8_lossy(&out.stdout)
                        .lines()
                        .next()
                        .map(|l| l.trim().to_string())
                });
                EngineStatus {
                    engine: name.to_string(),
                    available: true,
                    path: path.to_str().map(String::from),
                    version,
                }
            }
            None => EngineStatus {
                engine: name.to_string(),
                available: false,
                path: None,
                version: None,
            },
        };
        result.push(status);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_common_engines_in_sandbox() {
        // В песочнице есть ffmpeg и tesseract; тест пропускается, если нет.
        if which("ffmpeg").is_some() {
            assert!(find_engine("ffmpeg").is_some());
        }
        if which("soffice").is_some() {
            assert!(find_engine("soffice").is_some());
        }
    }

    #[test]
    fn missing_engine_is_error() {
        let err = require_engine("definitely-not-a-real-engine-xyz");
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("не найден"));
    }

    #[test]
    fn run_process_captures_output() {
        let out = run_process("python3", &["-c", "print('hello')"]).unwrap();
        assert!(String::from_utf8_lossy(&out.stdout).contains("hello"));
    }

    #[test]
    fn run_process_failure_has_stderr() {
        let err = run_process("python3", &["-c", "import sys; sys.exit(2)"]).unwrap_err();
        match err {
            ConvertError::EngineFailed { engine, code, .. } => {
                assert_eq!(engine, "python3");
                assert_eq!(code, 2);
            }
            other => panic!("ожидалась EngineFailed, получено {other:?}"),
        }
    }

    #[test]
    fn timeout_kills_hung_process() {
        use std::time::Instant;
        let started = Instant::now();
        let err = run_process_with_timeout(
            "python3",
            &["-c", "import time; time.sleep(60)"],
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(err.to_string().contains("таймаут"));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    fn which(name: &str) -> Option<PathBuf> {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(name))
                .find(|p| p.is_file())
        })
    }
}

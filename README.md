# CONVERT — универсальное настольное приложение для работы с файлами

Windows 10/11 desktop-приложение: конвертация, редактирование, анализ,
оптимизация и автоматизация работы с файлами. Основной режим — локальный.

## Структура проекта

```
convert/
├── crates/                  # Движки (Rust workspace)
│   ├── convert-core         # Форматы (magic bytes), матрица совместимости,
│   │                        #   ошибки, пути вывода, настройки, SHA-256
│   ├── convert-image        # Image Studio: конвертация, resize/crop/rotate,
│   │                        #   цветокоррекция, палитра, удаление фона, EXIF, SVG
│   ├── convert-media        # Video/Audio/Document/OCR Studio: обёртки
│   │                        #   ffmpeg/ffprobe, LibreOffice, Tesseract
│   ├── convert-pdf          # PDF Studio: merge/split/rotate, водяные знаки,
│   │                        #   нумерация, метаданные, текст, изображения↔PDF,
│   │                        #   searchable-PDF (hOCR), DOCX-сборка
│   ├── convert-archive      # ZIP (AES)/7Z (AES)/TAR/TAR.GZ: пароли,
│   │                        #   лимиты против zip-бомб, path traversal
│   ├── convert-db           # SQLite: история, очередь, настройки, пресеты
│   ├── convert-tools        # Дубликаты, QR, кодировки, File Info, Smart Compress,
│   │                        #   File Compare (пиксельный+diff), AI Assistant (rule-based),
│   │                        #   Quality Enhancer (уровни улучшения)
│   ├── convert-pipeline     # Batch: очередь с приоритетами, пауза, отмена, retry
│   └── convert-app          # Слой команд + Workflow engine + watch-папки (автосценарии)
├── src-tauri/               # Tauri 2 оболочка: команды, tray, окно
├── app/                     # Frontend: React + TypeScript + Vite + Tailwind
└── .github/workflows/       # CI: сборка Windows-установщика
```

## Запуск и сборка

Требования: Rust 1.75+, Node 20+, на Windows — WebView2 (ставится с Tauri).

```bash
# Тесты движков (132 теста, все локальные)
cargo test --workspace

# Разработка (desktop-окно)
npm --prefix app install
npx tauri dev          # из src-tauri: cargo tauri dev

# Установщик Windows
npx tauri build        # NSIS (.exe) + MSI в src-tauri/target/release/bundle/
```

Или через GitHub Actions: push тега `v*` → workflow соберёт установщик
и приложит его к релизу (см. .github/workflows/build-windows.yml).

## Внешние компоненты (опционально, устанавливаются отдельно)

| Компонент | Функция | Статус в приложении |
|---|---|---|
| FFmpeg/FFprobe | видео/аудио конвертация | проверяется на «Компонентах» в настройках |
| LibreOffice | офисные документы | путь ищется в Program Files |
| Tesseract OCR | распознавание текста | языки rus/eng ставятся отдельно |
| poppler (pdftoppm) | рендер PDF в изображения | для OCR сканов |

Без них соответствующие инструменты сообщают об отсутствии компонента
и предлагают установить — никаких скрытых падений.

## Что реализовано (без заглушек)

- Universal Converter: детект формата по содержимому, матрица совместимости
  с честными предупреждениями о потерях, рекомендации.
- Image Studio: JPG/PNG/WebP/GIF/BMP/TIFF/ICO/SVG, качество, DPI,
  resize (Fit/Cover/Exact), crop, повороты, яркость/контраст/насыщенность/
  гамма/автоконтраст, медианный фильтр, резкость, палитра (медианный срез),
  удаление однотонного фона (flood fill), чтение/правка/удаление EXIF.
- Video Studio: MP4/MKV/WebM, H.264/HEVC/VP9, CRF/битрейт, FPS, разрешение,
  обрезка, склейка (concat), извлечение аудио, кадры, скриншоты, ffprobe-анализ.
- Audio Studio: MP3/WAV/FLAC/OGG/Opus/M4A, битрейт, частота, каналы,
  громкость, нормализация EBU R128, обрезка.
- PDF Studio: слияние, разделение, extract/delete/reorder, повороты,
  водяные знаки, нумерация, метаданные (кириллица), извлечение текста,
  изображения→PDF, searchable-PDF после OCR.
- Document Studio: DOCX/ODT/RTF/XLSX/PPTX → PDF и между собой (LibreOffice).
- OCR Studio: Tesseract (локально), TXT/DOCX, searchable PDF.
- Archive Manager: ZIP/7Z с AES-паролями, TAR/TAR.GZ, листинг,
  защита от zip-бомб и path traversal.
- Инструменты: Format Detector, Duplicate Finder (размер→частичный→полный хэш),
  QR Generator, Text Encoding Converter (CP1251/KOI8-R/UTF-16...),
  File Information, Smart Compress (реальный анализ вариантов).
- Batch: очередь с приоритетами, пауза/продолжение, отмена, retry,
  параллельные воркеры, история каждой операции в SQLite.
- File Compare: до/после со слайдером (изображения, с % попиксельной
  разницы), построчный diff текстов, сравнение кодеков/битрейтов медиа.
- AI File Assistant: локальный rule-based помощник («уменьши видео» → план +
  готовые задачи после подтверждения), без облака.
- Workflow Builder: цепочки шагов (фильтр/сжатие/конвертация/resize/OCR/ZIP),
  сохранение в БД, повторный запуск.
- Автосценарии: watch-папки — новые файлы предлагаются к обработке (после
  подтверждения, как требует ТЗ).
- Metadata Editor: EXIF просмотр/правка/удаление с предупреждением, палитра.
- Windows-интеграция: контекстное меню проводника «Открыть в CONVERT»,
  «Открыть с помощью», системный трей (NSIS-хуки, HKCU).
- UI: 14 страниц (Dashboard c Smart Scan, 7 студий, Сравнение, AI-помощник,
  Сценарии, Инструменты, История, Настройки), темы, акцент, тосты.

## Ограничения (честно)

- HEIC/AVIF: конвертация через FFmpeg (если собран с libaom); чистый
  Rust-движок их не декодирует.
- RAR: чтение не поддержано (лицензия unrar); конвертер предлагает
  установить внешний инструмент в будущем.
- Сложное удаление фона (волосы/градиенты) требует ML-модуля — в базовой
  версии работает алгоритмический метод для однотонных фонов.
- RUS-язык Tesseract и poppler устанавливаются отдельно (см. таблицу выше).
- PDF-шифрование паролем не реализовано (lopdf не поддерживает запись шифрования).
- Глобальные горячие клавиши, системные уведомления Windows, предпрослушивание
  аудио и плеер видео в UI — не реализованы.
- Сборка Windows-установщика — на Windows-машине или через GitHub Actions
  (см. ГАЙД-СБОРКА-И-ПУБЛИКАЦИЯ.md).

//! convert-tools — каталог дополнительных инструментов и интеллектуальные
//! функции (ТЗ 4.11, 5.2, 4.3, 7):
//! Format Detector, Duplicate Finder, QR Generator, Text Encoding
//! Converter, File Information, File Compare, Smart Compress,
//! AI File Assistant (rule-based, локальный), Quality Enhancer.

pub mod assistant;
pub mod compare;
pub mod duplicates;
pub mod encoding;
pub mod enhance;
pub mod file_info;
pub mod qr;
pub mod smart_compress;

pub use assistant::{ask, AssistantReply, ProposedTask};
pub use compare::{compare_files, text_diff, CompareKind, CompareReport, DiffChange, DiffLine};
pub use duplicates::{find_duplicates, DuplicateGroup};
pub use encoding::{convert_encoding, detect_encoding, EncodingName};
pub use enhance::{enhance, EnhanceLevel, EnhanceResult};
pub use file_info::{file_information, FileInfo};
pub use qr::{generate_qr_png, QrStyle};
pub use smart_compress::{analyze_image, smart_compress_image, CompressGoal, SmartCompressPlan};

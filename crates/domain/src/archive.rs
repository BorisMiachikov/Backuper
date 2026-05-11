use serde::{Deserialize, Serialize};

use crate::vault::SecretRef;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    Zip,
    SevenZ,
    ZstdTar,
}

impl ArchiveFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::SevenZ => "7z",
            Self::ZstdTar => "tar.zst",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Encryption {
    None,
    Aes256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveConfig {
    pub format: ArchiveFormat,
    /// 0..=22 (zstd) or 0..=9 (zip/deflate). Domain-уровень: 0..=22, конкретные адаптеры клампят.
    pub compression_level: u8,
    pub password_ref: Option<SecretRef>,
    pub encryption: Encryption,
    /// 0 — без разбиения.
    pub volume_size_mb: u32,
    pub verify_after_create: bool,
    pub name_template: String,
}

impl Default for ArchiveConfig {
    fn default() -> Self {
        Self {
            format: ArchiveFormat::ZstdTar,
            compression_level: 6,
            password_ref: None,
            encryption: Encryption::None,
            volume_size_mb: 0,
            verify_after_create: true,
            name_template: "{source}-{yyyy}{MM}{dd}-{HH}{mm}{ss}.{ext}".into(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExcludeRules {
    pub masks: Vec<String>,
    pub dirs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub keep_last: Option<u32>,
    pub max_total_gb: Option<u32>,
    pub min_age_days: Option<u32>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            keep_last: Some(14),
            max_total_gb: None,
            min_age_days: None,
        }
    }
}

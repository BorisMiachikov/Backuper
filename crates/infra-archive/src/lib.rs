//! Архивирование: zip, 7z, zstd-tar.
//!
//! Stage 0: только трейт `Archiver` и заглушки. Реальные реализации (zip с AES,
//! zstd-tar streaming, sha256-верификация) — Stage 2.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use domain::ArchiveConfig;

#[derive(Debug, thiserror::Error)]
pub enum ArchiverError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported format")]
    UnsupportedFormat,
    #[error("integrity check failed")]
    IntegrityFailed,
    #[error("other: {0}")]
    Other(String),
}

#[derive(Debug, Clone)]
pub struct ArchiveResult {
    pub path: PathBuf,
    pub size: u64,
    pub sha256_hex: String,
}

#[async_trait]
pub trait Archiver: Send + Sync {
    async fn create(
        &self,
        files: &[PathBuf],
        out: &Path,
        cfg: &ArchiveConfig,
    ) -> Result<ArchiveResult, ArchiverError>;

    async fn verify(&self, archive: &Path) -> Result<(), ArchiverError>;
}

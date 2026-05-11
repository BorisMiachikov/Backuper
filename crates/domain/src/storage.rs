use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StorageKind {
    Local,
    Smb,
    YaDisk,
    GDrive,
}

impl StorageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Smb => "smb",
            Self::YaDisk => "yadisk",
            Self::GDrive => "gdrive",
        }
    }
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("connection failed: {0}")]
    Connection(String),
    #[error("authentication required")]
    Unauthorized,
    #[error("quota exceeded")]
    QuotaExceeded,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cancelled")]
    Cancelled,
    #[error("other: {0}")]
    Other(String),
}

impl StorageError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Connection(_) | Self::QuotaExceeded | Self::Io(_)
        )
    }
}

#[derive(Debug, Clone)]
pub struct UploadProgress {
    pub bytes: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct RemoteEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: Option<time::OffsetDateTime>,
}

#[async_trait]
pub trait Storage: Send + Sync {
    fn kind(&self) -> StorageKind;

    async fn check_connection(&self) -> Result<(), StorageError>;

    async fn upload(
        &self,
        local: &Path,
        remote: &str,
        progress: Option<mpsc::Sender<UploadProgress>>,
    ) -> Result<u64, StorageError>;

    async fn delete(&self, remote: &str) -> Result<(), StorageError>;

    async fn list(&self, dir: &str) -> Result<Vec<RemoteEntry>, StorageError>;

    async fn stat(&self, remote: &str) -> Result<RemoteEntry, StorageError>;
}

//! Google Drive REST + OAuth2 PKCE. Stage 0 — заглушка-скелет.

use std::path::Path;

use async_trait::async_trait;
use domain::{RemoteEntry, Storage, StorageError, StorageKind, UploadProgress};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct GDriveClient {
    pub base_url: String,
}

impl Default for GDriveClient {
    fn default() -> Self {
        Self {
            base_url: "https://www.googleapis.com".into(),
        }
    }
}

#[async_trait]
impl Storage for GDriveClient {
    fn kind(&self) -> StorageKind {
        StorageKind::GDrive
    }

    async fn check_connection(&self) -> Result<(), StorageError> {
        Err(StorageError::Other("gdrive stage 0 stub".into()))
    }

    async fn upload(
        &self,
        _local: &Path,
        _remote: &str,
        _progress: Option<mpsc::Sender<UploadProgress>>,
    ) -> Result<u64, StorageError> {
        Err(StorageError::Other("gdrive stage 0 stub".into()))
    }

    async fn delete(&self, _remote: &str) -> Result<(), StorageError> {
        Err(StorageError::Other("gdrive stage 0 stub".into()))
    }

    async fn list(&self, _dir: &str) -> Result<Vec<RemoteEntry>, StorageError> {
        Err(StorageError::Other("gdrive stage 0 stub".into()))
    }

    async fn stat(&self, _remote: &str) -> Result<RemoteEntry, StorageError> {
        Err(StorageError::Other("gdrive stage 0 stub".into()))
    }
}

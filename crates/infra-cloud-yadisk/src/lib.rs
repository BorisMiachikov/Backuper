//! Яндекс.Диск REST + OAuth2. Stage 0 — заглушка-скелет.

use std::path::Path;

use async_trait::async_trait;
use domain::{RemoteEntry, Storage, StorageError, StorageKind, UploadProgress};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct YaDiskClient {
    pub base_url: String,
}

impl Default for YaDiskClient {
    fn default() -> Self {
        Self {
            base_url: "https://cloud-api.yandex.net/v1/disk".into(),
        }
    }
}

#[async_trait]
impl Storage for YaDiskClient {
    fn kind(&self) -> StorageKind {
        StorageKind::YaDisk
    }

    async fn check_connection(&self) -> Result<(), StorageError> {
        Err(StorageError::Other("yadisk stage 0 stub".into()))
    }

    async fn upload(
        &self,
        _local: &Path,
        _remote: &str,
        _progress: Option<mpsc::Sender<UploadProgress>>,
    ) -> Result<u64, StorageError> {
        Err(StorageError::Other("yadisk stage 0 stub".into()))
    }

    async fn delete(&self, _remote: &str) -> Result<(), StorageError> {
        Err(StorageError::Other("yadisk stage 0 stub".into()))
    }

    async fn list(&self, _dir: &str) -> Result<Vec<RemoteEntry>, StorageError> {
        Err(StorageError::Other("yadisk stage 0 stub".into()))
    }

    async fn stat(&self, _remote: &str) -> Result<RemoteEntry, StorageError> {
        Err(StorageError::Other("yadisk stage 0 stub".into()))
    }
}

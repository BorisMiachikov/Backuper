//! Локальные FS-операции и SMB/UNC через стандартный путь Windows.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use domain::{RemoteEntry, Storage, StorageError, StorageKind, UploadProgress};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct LocalStorage {
    pub root: PathBuf,
}

impl LocalStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, remote: &str) -> PathBuf {
        self.root.join(remote.trim_start_matches(['/', '\\']))
    }
}

#[async_trait]
impl Storage for LocalStorage {
    fn kind(&self) -> StorageKind {
        StorageKind::Local
    }

    async fn check_connection(&self) -> Result<(), StorageError> {
        tokio::fs::create_dir_all(&self.root).await?;
        Ok(())
    }

    async fn upload(
        &self,
        local: &Path,
        remote: &str,
        progress: Option<mpsc::Sender<UploadProgress>>,
    ) -> Result<u64, StorageError> {
        let dst = self.resolve(remote);
        if let Some(parent) = dst.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let meta = tokio::fs::metadata(local).await?;
        let total = Some(meta.len());

        let mut src = tokio::fs::File::open(local).await?;
        let mut dst_f = tokio::fs::File::create(&dst).await?;
        let mut buf = vec![0u8; 1 << 20]; // 1 MiB
        let mut copied: u64 = 0;
        loop {
            use tokio::io::AsyncReadExt;
            let n = src.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            dst_f.write_all(&buf[..n]).await?;
            copied += n as u64;
            if let Some(tx) = &progress {
                let _ = tx
                    .send(UploadProgress {
                        bytes: copied,
                        total,
                    })
                    .await;
            }
        }
        dst_f.flush().await?;
        Ok(copied)
    }

    async fn delete(&self, remote: &str) -> Result<(), StorageError> {
        let path = self.resolve(remote);
        if path.is_dir() {
            tokio::fs::remove_dir_all(path).await?;
        } else {
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }

    async fn list(&self, dir: &str) -> Result<Vec<RemoteEntry>, StorageError> {
        let path = self.resolve(dir);
        let mut rd = tokio::fs::read_dir(&path).await?;
        let mut out = Vec::new();
        while let Some(entry) = rd.next_entry().await? {
            let meta = entry.metadata().await?;
            let modified: Option<time::OffsetDateTime> =
                meta.modified().ok().and_then(|m| m.try_into().ok());
            out.push(RemoteEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                size: meta.len(),
                is_dir: meta.is_dir(),
                modified,
            });
        }
        Ok(out)
    }

    async fn stat(&self, remote: &str) -> Result<RemoteEntry, StorageError> {
        let path = self.resolve(remote);
        let meta = tokio::fs::metadata(&path).await?;
        let modified: Option<time::OffsetDateTime> =
            meta.modified().ok().and_then(|m| m.try_into().ok());
        Ok(RemoteEntry {
            name: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            size: meta.len(),
            is_dir: meta.is_dir(),
            modified,
        })
    }
}

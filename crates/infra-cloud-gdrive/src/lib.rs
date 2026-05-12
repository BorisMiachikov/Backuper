//! Google Drive REST API v3 (Bearer-токен).
//!
//! Поскольку Google Drive не имеет понятия «путь» — файлы идентифицируются
//! по ID — операции list/delete/stat резолвят ID по имени файла (первое совпадение).
//! Для upload используется multipart/related загрузка в корень диска.

use std::path::Path;

use async_trait::async_trait;
use domain::{RemoteEntry, Storage, StorageError, StorageKind, UploadProgress};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct GDriveClient {
    token: String,
    client: Client,
    base_url: String,
}

impl GDriveClient {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            client: Client::new(),
            base_url: "https://www.googleapis.com".into(),
        }
    }

    fn bearer(&self) -> String {
        format!("Bearer {}", self.token)
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<DriveFile>, StorageError> {
        let url = format!("{}/drive/v3/files", self.base_url);
        let q = format!("name='{}' and trashed=false", name.replace('\'', "\\'"));
        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.bearer())
            .query(&[
                ("q", q.as_str()),
                ("fields", "files(id,name,size,modifiedTime)"),
                ("pageSize", "10"),
            ])
            .send()
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;

        match resp.status() {
            StatusCode::UNAUTHORIZED => return Err(StorageError::Unauthorized),
            s if !s.is_success() => {
                return Err(StorageError::Connection(format!("find_by_name failed: {s}")))
            }
            _ => {}
        }

        let list: FileList = resp
            .json()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;

        Ok(list.files.into_iter().next())
    }
}

#[derive(Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    #[serde(default)]
    size: Option<String>,
    #[serde(rename = "modifiedTime", default)]
    modified_time: Option<String>,
}

#[derive(Deserialize)]
struct FileList {
    #[serde(default)]
    files: Vec<DriveFile>,
}

fn parse_gd_time(s: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
}

fn file_to_entry(f: DriveFile) -> RemoteEntry {
    RemoteEntry {
        name: f.name,
        size: f.size.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0),
        is_dir: false,
        modified: f.modified_time.as_deref().and_then(parse_gd_time),
    }
}

#[async_trait]
impl Storage for GDriveClient {
    fn kind(&self) -> StorageKind {
        StorageKind::GDrive
    }

    async fn check_connection(&self) -> Result<(), StorageError> {
        let url = format!("{}/drive/v3/about", self.base_url);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.bearer())
            .query(&[("fields", "user")])
            .send()
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;

        match resp.status() {
            StatusCode::UNAUTHORIZED => Err(StorageError::Unauthorized),
            s if s.is_success() => Ok(()),
            s => Err(StorageError::Connection(format!("unexpected status: {s}"))),
        }
    }

    async fn upload(
        &self,
        local: &Path,
        remote: &str,
        _progress: Option<mpsc::Sender<UploadProgress>>,
    ) -> Result<u64, StorageError> {
        let file_name = Path::new(remote)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(remote)
            .to_owned();

        let data = tokio::fs::read(local).await?;
        let size = data.len() as u64;

        // Построим multipart/related тело вручную.
        let boundary = "backuper_gdrive_boundary_a3f9";
        let metadata = serde_json::json!({"name": file_name}).to_string();

        let mut body: Vec<u8> = Vec::new();
        use std::io::Write;
        write!(
            body,
            "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{metadata}\r\n\
             --{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .expect("write to vec");
        body.extend_from_slice(&data);
        write!(body, "\r\n--{boundary}--").expect("write to vec");

        let url = format!("{}/upload/drive/v3/files", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", self.bearer())
            .header(
                "Content-Type",
                format!("multipart/related; boundary={boundary}"),
            )
            .query(&[("uploadType", "multipart")])
            .body(body)
            .send()
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;

        match resp.status() {
            StatusCode::OK | StatusCode::CREATED => {
                debug!(remote, size, "gdrive: uploaded");
                Ok(size)
            }
            StatusCode::UNAUTHORIZED => Err(StorageError::Unauthorized),
            s => {
                let body = resp.text().await.unwrap_or_default();
                Err(StorageError::Connection(format!("upload failed {s}: {body}")))
            }
        }
    }

    async fn delete(&self, remote: &str) -> Result<(), StorageError> {
        let file_name = Path::new(remote)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(remote);

        let file = self
            .find_by_name(file_name)
            .await?
            .ok_or_else(|| StorageError::NotFound(remote.into()))?;

        let url = format!("{}/drive/v3/files/{}", self.base_url, file.id);
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", self.bearer())
            .send()
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;

        match resp.status() {
            StatusCode::NO_CONTENT | StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED => Err(StorageError::Unauthorized),
            StatusCode::NOT_FOUND => Err(StorageError::NotFound(remote.into())),
            s => {
                warn!(remote, %s, "gdrive: delete failed");
                Err(StorageError::Connection(format!("delete failed: {s}")))
            }
        }
    }

    async fn list(&self, _dir: &str) -> Result<Vec<RemoteEntry>, StorageError> {
        let url = format!("{}/drive/v3/files", self.base_url);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.bearer())
            .query(&[
                ("q", "trashed=false"),
                ("fields", "files(id,name,size,modifiedTime)"),
                ("pageSize", "1000"),
            ])
            .send()
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;

        match resp.status() {
            StatusCode::UNAUTHORIZED => return Err(StorageError::Unauthorized),
            s if !s.is_success() => {
                return Err(StorageError::Connection(format!("list failed: {s}")))
            }
            _ => {}
        }

        let list: FileList = resp
            .json()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;

        Ok(list.files.into_iter().map(file_to_entry).collect())
    }

    async fn stat(&self, remote: &str) -> Result<RemoteEntry, StorageError> {
        let file_name = Path::new(remote)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(remote);

        let file = self
            .find_by_name(file_name)
            .await?
            .ok_or_else(|| StorageError::NotFound(remote.into()))?;

        Ok(file_to_entry(file))
    }
}

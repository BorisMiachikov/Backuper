//! Яндекс.Диск REST API (OAuth-токен).

use std::path::Path;

use async_trait::async_trait;
use domain::{RemoteEntry, Storage, StorageError, StorageKind, UploadProgress};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct YaDiskClient {
    token: String,
    client: Client,
    base_url: String,
}

impl YaDiskClient {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            client: Client::new(),
            base_url: "https://cloud-api.yandex.net/v1/disk".into(),
        }
    }

    fn auth(&self) -> String {
        format!("OAuth {}", self.token)
    }
}

#[derive(Deserialize)]
struct UploadLink {
    href: String,
}

#[derive(Deserialize)]
struct ResourceList {
    #[serde(rename = "_embedded")]
    embedded: Embedded,
}

#[derive(Deserialize)]
struct Embedded {
    items: Vec<ResourceItem>,
}

#[derive(Deserialize)]
struct ResourceItem {
    name: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    modified: Option<String>,
}

fn parse_ya_time(s: &str) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
}

#[async_trait]
impl Storage for YaDiskClient {
    fn kind(&self) -> StorageKind {
        StorageKind::YaDisk
    }

    async fn check_connection(&self) -> Result<(), StorageError> {
        let resp = self
            .client
            .get(&self.base_url)
            .header("Authorization", self.auth())
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
        let link_url = format!("{}/resources/upload", self.base_url);
        let resp = self
            .client
            .get(&link_url)
            .header("Authorization", self.auth())
            .query(&[("path", remote), ("overwrite", "true")])
            .send()
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;

        match resp.status() {
            StatusCode::UNAUTHORIZED => return Err(StorageError::Unauthorized),
            s if !s.is_success() => {
                let body = resp.text().await.unwrap_or_default();
                return Err(StorageError::Connection(format!(
                    "get upload link {s}: {body}"
                )));
            }
            _ => {}
        }

        let link: UploadLink = resp
            .json()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;

        let data = tokio::fs::read(local).await?;
        let size = data.len() as u64;

        let put = self
            .client
            .put(&link.href)
            .body(data)
            .send()
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;

        match put.status() {
            StatusCode::CREATED | StatusCode::OK | StatusCode::ACCEPTED => {
                debug!(remote, size, "yadisk: uploaded");
                Ok(size)
            }
            s => Err(StorageError::Connection(format!("put failed: {s}"))),
        }
    }

    async fn delete(&self, remote: &str) -> Result<(), StorageError> {
        let url = format!("{}/resources", self.base_url);
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", self.auth())
            .query(&[("path", remote), ("permanently", "true")])
            .send()
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;

        match resp.status() {
            StatusCode::NO_CONTENT | StatusCode::OK | StatusCode::ACCEPTED => Ok(()),
            StatusCode::UNAUTHORIZED => Err(StorageError::Unauthorized),
            StatusCode::NOT_FOUND => Err(StorageError::NotFound(remote.into())),
            s => {
                warn!(remote, %s, "yadisk: delete failed");
                Err(StorageError::Connection(format!("delete failed: {s}")))
            }
        }
    }

    async fn list(&self, dir: &str) -> Result<Vec<RemoteEntry>, StorageError> {
        let url = format!("{}/resources", self.base_url);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.auth())
            .query(&[("path", dir)])
            .send()
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;

        match resp.status() {
            StatusCode::UNAUTHORIZED => return Err(StorageError::Unauthorized),
            StatusCode::NOT_FOUND => return Ok(vec![]),
            s if !s.is_success() => {
                return Err(StorageError::Connection(format!("list failed: {s}")))
            }
            _ => {}
        }

        let res: ResourceList = resp
            .json()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;

        Ok(res
            .embedded
            .items
            .into_iter()
            .map(|item| RemoteEntry {
                name: item.name,
                size: item.size.unwrap_or(0),
                is_dir: item.kind == "dir",
                modified: item.modified.as_deref().and_then(parse_ya_time),
            })
            .collect())
    }

    async fn stat(&self, remote: &str) -> Result<RemoteEntry, StorageError> {
        let url = format!("{}/resources", self.base_url);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.auth())
            .query(&[("path", remote)])
            .send()
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;

        match resp.status() {
            StatusCode::UNAUTHORIZED => return Err(StorageError::Unauthorized),
            StatusCode::NOT_FOUND => return Err(StorageError::NotFound(remote.into())),
            s if !s.is_success() => {
                return Err(StorageError::Connection(format!("stat failed: {s}")))
            }
            _ => {}
        }

        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;

        Ok(RemoteEntry {
            name: v["name"].as_str().unwrap_or("").to_owned(),
            size: v["size"].as_u64().unwrap_or(0),
            is_dir: v["type"].as_str() == Some("dir"),
            modified: v["modified"].as_str().and_then(parse_ya_time),
        })
    }
}

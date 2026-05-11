use async_trait::async_trait;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Логическая ссылка на секрет внутри vault'а.
///
/// Конвенция именования: `"<kind>::<owner_id>"`, например:
/// `"archive::01J9PZ7XKQF"` или `"gdrive::01J9PZB12AB"`.
pub type SecretRef = String;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretKind {
    ArchivePassword,
    OAuthToken,
    SmbCredentials,
    Generic,
}

#[derive(Debug, Clone)]
pub struct SecretValue {
    pub kind: SecretKind,
    pub payload: SecretString,
    pub created_at: OffsetDateTime,
    pub rotated_at: Option<OffsetDateTime>,
}

#[async_trait]
pub trait SecretVault: Send + Sync {
    async fn put(&self, r#ref: &str, kind: SecretKind, payload: SecretString)
        -> Result<(), VaultError>;

    async fn get(&self, r#ref: &str) -> Result<SecretValue, VaultError>;

    async fn delete(&self, r#ref: &str) -> Result<(), VaultError>;

    async fn list_refs(&self) -> Result<Vec<SecretRef>, VaultError>;
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("backend error: {0}")]
    Backend(String),
    #[error("decryption failed")]
    Decryption,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

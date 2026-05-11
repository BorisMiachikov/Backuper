use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use domain::vault::{SecretKind, SecretRef, SecretValue, SecretVault, VaultError};
use parking_lot::RwLock;
use secrecy::{ExposeSecret, SecretString};
use time::OffsetDateTime;

/// Без шифрования, в памяти. Только для dev/тестов. Не использовать в проде.
#[derive(Default, Clone)]
pub struct InMemoryVault {
    inner: Arc<RwLock<HashMap<String, StoredSecret>>>,
}

#[derive(Clone)]
struct StoredSecret {
    kind: SecretKind,
    payload: String,
    created_at: OffsetDateTime,
    rotated_at: Option<OffsetDateTime>,
}

impl InMemoryVault {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SecretVault for InMemoryVault {
    async fn put(
        &self,
        r#ref: &str,
        kind: SecretKind,
        payload: SecretString,
    ) -> Result<(), VaultError> {
        let now = OffsetDateTime::now_utc();
        let mut guard = self.inner.write();
        let existing = guard.get(r#ref).cloned();
        guard.insert(
            r#ref.to_owned(),
            StoredSecret {
                kind,
                payload: payload.expose_secret().to_owned(),
                created_at: existing.as_ref().map(|s| s.created_at).unwrap_or(now),
                rotated_at: existing.map(|_| now),
            },
        );
        Ok(())
    }

    async fn get(&self, r#ref: &str) -> Result<SecretValue, VaultError> {
        let guard = self.inner.read();
        let stored = guard
            .get(r#ref)
            .ok_or_else(|| VaultError::NotFound(r#ref.to_owned()))?;
        Ok(SecretValue {
            kind: stored.kind,
            payload: SecretString::from(stored.payload.clone()),
            created_at: stored.created_at,
            rotated_at: stored.rotated_at,
        })
    }

    async fn delete(&self, r#ref: &str) -> Result<(), VaultError> {
        self.inner.write().remove(r#ref);
        Ok(())
    }

    async fn list_refs(&self) -> Result<Vec<SecretRef>, VaultError> {
        Ok(self.inner.read().keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_and_get_roundtrip() {
        let v = InMemoryVault::new();
        v.put(
            "archive::x",
            SecretKind::ArchivePassword,
            SecretString::from("hunter2".to_owned()),
        )
        .await
        .unwrap();
        let got = v.get("archive::x").await.unwrap();
        assert_eq!(got.payload.expose_secret(), "hunter2");
    }
}

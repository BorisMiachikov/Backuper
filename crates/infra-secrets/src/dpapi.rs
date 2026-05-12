//! Windows DPAPI wrapper + file-backed DpapiVault.
//!
//! `protect`/`unprotect` — низкоуровневые функции DPAPI (CryptProtectData / CryptUnprotectData).
//! `DpapiVault` — полноценный SecretVault: хранит JSON-словарь секретов,
//!   зашифрованный DPAPI, в файле `vault.dat` внутри data_dir.

#![cfg(windows)]

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

use async_trait::async_trait;
use domain::vault::{SecretKind, SecretRef, SecretValue, SecretVault, VaultError};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::Mutex;
use tracing::{debug, warn};
use windows::Win32::Foundation::LocalFree;
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
};

// ── DPAPI low-level ──────────────────────────────────────────────────────────

/// Шифрует `plain` через DPAPI (user-scope).
pub fn protect(plain: &[u8]) -> io::Result<Vec<u8>> {
    let mut in_buf = plain.to_vec();
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: in_buf.len() as u32,
        pbData: in_buf.as_mut_ptr(),
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptProtectData(&in_blob, None, None, None, None, 0, &mut out_blob)
            .map_err(|e| io::Error::other(format!("CryptProtectData: {e}")))?;
    }

    let result =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();

    unsafe {
        let _ = LocalFree(windows::Win32::Foundation::HLOCAL(out_blob.pbData as _));
    }

    Ok(result)
}

/// Дешифрует `enc` через DPAPI (user-scope).
pub fn unprotect(enc: &[u8]) -> io::Result<Vec<u8>> {
    let mut in_buf = enc.to_vec();
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: in_buf.len() as u32,
        pbData: in_buf.as_mut_ptr(),
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptUnprotectData(&in_blob, None, None, None, None, 0, &mut out_blob)
            .map_err(|e| io::Error::other(format!("CryptUnprotectData: {e}")))?;
    }

    let result =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();

    unsafe {
        let _ = LocalFree(windows::Win32::Foundation::HLOCAL(out_blob.pbData as _));
    }

    Ok(result)
}

// ── DpapiVault ───────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
struct StoredEntry {
    kind: SecretKind,
    payload: String,
    created_at: OffsetDateTime,
    rotated_at: Option<OffsetDateTime>,
}

#[derive(Serialize, Deserialize, Default)]
struct VaultFile {
    secrets: HashMap<String, StoredEntry>,
}

/// Персистентный vault: хранит секреты в DPAPI-зашифрованном JSON-файле.
///
/// Файл читается при `open()`, все операции держат зеркало в RAM
/// (tokio::sync::Mutex) и сбрасывают изменения на диск при каждом `put`/`delete`.
pub struct DpapiVault {
    path: PathBuf,
    cache: Mutex<HashMap<String, StoredEntry>>,
}

impl DpapiVault {
    /// Открыть vault по пути `path`.
    /// Если файл не существует — создаётся пустой vault (файл появится при первом `put`).
    pub async fn open(path: PathBuf) -> Result<Self, VaultError> {
        let cache = if path.exists() {
            let enc = tokio::fs::read(&path).await?;
            let plain = tokio::task::spawn_blocking(move || unprotect(&enc))
                .await
                .map_err(|e| VaultError::Backend(e.to_string()))?
                .map_err(|e| VaultError::Backend(e.to_string()))?;
            let file: VaultFile = serde_json::from_slice(&plain).unwrap_or_default();
            debug!(count = file.secrets.len(), "dpapi vault loaded");
            file.secrets
        } else {
            debug!("dpapi vault file not found, starting empty");
            HashMap::new()
        };
        Ok(Self {
            path,
            cache: Mutex::new(cache),
        })
    }

    async fn persist(&self, cache: &HashMap<String, StoredEntry>) -> Result<(), VaultError> {
        let file = VaultFile {
            secrets: cache.clone(),
        };
        let json = serde_json::to_vec(&file).map_err(|e| VaultError::Backend(e.to_string()))?;
        let enc = tokio::task::spawn_blocking(move || protect(&json))
            .await
            .map_err(|e| VaultError::Backend(e.to_string()))?
            .map_err(|e| VaultError::Backend(e.to_string()))?;
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.path, &enc).await?;
        Ok(())
    }
}

#[async_trait]
impl SecretVault for DpapiVault {
    async fn put(
        &self,
        r#ref: &str,
        kind: SecretKind,
        payload: SecretString,
    ) -> Result<(), VaultError> {
        let mut cache = self.cache.lock().await;
        let now = OffsetDateTime::now_utc();
        let existing = cache.get(r#ref).cloned();
        cache.insert(
            r#ref.to_owned(),
            StoredEntry {
                kind,
                payload: payload.expose_secret().to_owned(),
                created_at: existing.as_ref().map(|s| s.created_at).unwrap_or(now),
                rotated_at: existing.map(|_| now),
            },
        );
        self.persist(&cache).await
    }

    async fn get(&self, r#ref: &str) -> Result<SecretValue, VaultError> {
        let cache = self.cache.lock().await;
        let entry = cache
            .get(r#ref)
            .ok_or_else(|| VaultError::NotFound(r#ref.to_owned()))?;
        Ok(SecretValue {
            kind: entry.kind,
            payload: SecretString::from(entry.payload.clone()),
            created_at: entry.created_at,
            rotated_at: entry.rotated_at,
        })
    }

    async fn delete(&self, r#ref: &str) -> Result<(), VaultError> {
        let mut cache = self.cache.lock().await;
        if cache.remove(r#ref).is_none() {
            warn!(r#ref, "dpapi vault: delete called for unknown ref");
        }
        self.persist(&cache).await
    }

    async fn list_refs(&self) -> Result<Vec<SecretRef>, VaultError> {
        Ok(self.cache.lock().await.keys().cloned().collect())
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protect_roundtrip() {
        let original = b"super-secret-payload";
        let enc = protect(original).expect("protect");
        assert_ne!(enc, original);
        let dec = unprotect(&enc).expect("unprotect");
        assert_eq!(dec, original);
    }

    #[tokio::test]
    async fn vault_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.dat");
        let vault = DpapiVault::open(path.clone()).await.unwrap();

        vault
            .put(
                "storage::test",
                SecretKind::OAuthToken,
                SecretString::from("my-token".to_owned()),
            )
            .await
            .unwrap();

        let sv = vault.get("storage::test").await.unwrap();
        assert_eq!(sv.payload.expose_secret(), "my-token");

        // Reload from disk — must survive round-trip.
        let vault2 = DpapiVault::open(path).await.unwrap();
        let sv2 = vault2.get("storage::test").await.unwrap();
        assert_eq!(sv2.payload.expose_secret(), "my-token");
    }
}

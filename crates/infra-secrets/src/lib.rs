//! Хранилище секретов с DPAPI (Windows) и in-memory fallback (для тестов / Linux).
//!
//! Уровни защиты:
//! - master_key (32 байта) генерируется случайно и хранится зашифрованным DPAPI в файле `vault.key`;
//! - содержимое vault (sled-like JSON-файл) шифруется AES-GCM на этом master_key;
//! - в RAM ключ держится в `secrecy::SecretBox<[u8; 32]>` (zeroize-on-drop).
//!
//! Stage 0: реализован in-memory backend (для разработки и тестов). Stage 5 — реальный DPAPI.

#[cfg(windows)]
pub mod dpapi;

pub mod memory;

pub use memory::InMemoryVault;

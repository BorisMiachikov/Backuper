//! Хранилище секретов: `DpapiVault` (Windows, файл + DPAPI) и `InMemoryVault` (тесты).

#[cfg(windows)]
pub mod dpapi;

pub mod memory;

pub use memory::InMemoryVault;
#[cfg(windows)]
pub use dpapi::DpapiVault;

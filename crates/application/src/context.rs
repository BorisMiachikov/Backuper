use std::sync::Arc;

use domain::{
    DomainEvent, JobRepository, Notifier, OneCRunner, SecretVault, SettingsRepository,
    SourceRepository, Storage, StorageRepository,
};
use tokio::sync::broadcast;

use crate::clock::Clock;

/// Контейнер всех зависимостей приложения.
///
/// Собирается один раз на старте через `bootstrap` в crate `app` и передаётся в
/// планировщик, pipeline и handlers как `Arc<AppContext>`.
#[derive(Clone)]
pub struct AppContext {
    pub sources: Arc<dyn SourceRepository>,
    pub jobs: Arc<dyn JobRepository>,
    pub storages: Arc<dyn StorageRepository>,
    pub settings: Arc<dyn SettingsRepository>,
    pub storage_registry: Arc<StorageRegistry>,
    pub vault: Arc<dyn SecretVault>,
    pub onec: Arc<dyn OneCRunner>,
    pub notifier: Arc<dyn Notifier>,
    pub clock: Arc<dyn Clock>,
    pub events: broadcast::Sender<DomainEvent>,
}

/// Реестр живых экземпляров `Storage` (один на каждый storage-row в БД).
/// Конкретная реализация хранится за `dyn Storage`; ключ — `Uuid` storage'а.
#[derive(Default)]
pub struct StorageRegistry {
    inner: dashmap::DashMap<uuid::Uuid, Arc<dyn Storage>>,
}

impl StorageRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, id: uuid::Uuid, storage: Arc<dyn Storage>) {
        self.inner.insert(id, storage);
    }

    pub fn get(&self, id: uuid::Uuid) -> Option<Arc<dyn Storage>> {
        self.inner.get(&id).map(|v| v.clone())
    }

    pub fn remove(&self, id: uuid::Uuid) {
        self.inner.remove(&id);
    }
}

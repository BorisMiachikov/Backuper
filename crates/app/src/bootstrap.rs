//! Корень dependency injection: собирает `AppContext` из реальных адаптеров.

use std::sync::Arc;

use application::clock::SystemClock;
use application::context::{AppContext, StorageRegistry};
use domain::{StorageDescriptor, StorageKind, StorageRepository};
use infra_1c::{DefaultOneCRunner, OneCConfig};
use infra_fs::LocalStorage;
use infra_secrets::InMemoryVault;
use infra_storage_sqlite::{
    SqliteJobRepository, SqliteSettingsRepository, SqliteSourceRepository, SqliteStorageRepository,
};
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::notifications::SystemNotifier;

pub async fn build_context() -> anyhow::Result<AppContext> {
    let db_path = shared::paths::app_db_path();
    info!(db = %db_path.display(), "opening sqlite pool");
    let pool = infra_storage_sqlite::open_app_db(&db_path).await?;

    let sources = Arc::new(SqliteSourceRepository::new(pool.clone()));
    let jobs = Arc::new(SqliteJobRepository::new(pool.clone()));
    let storages = Arc::new(SqliteStorageRepository::new(pool.clone()));
    let settings = Arc::new(SqliteSettingsRepository::new(pool.clone()));

    // Восстановление после краша: помечаем все running/pending runs как interrupted.
    use domain::JobRepository;
    match jobs.mark_running_as_interrupted().await {
        Ok(0) => {}
        Ok(n) => tracing::info!(interrupted = n, "recovered interrupted runs"),
        Err(e) => tracing::warn!(error = %e, "could not mark interrupted runs"),
    }

    // Заполняем StorageRegistry по записям из БД.
    let registry = Arc::new(StorageRegistry::new());
    match storages.list().await {
        Ok(descs) => {
            for desc in &descs {
                if !desc.enabled {
                    continue;
                }
                match make_storage(desc) {
                    Ok(s) => registry.register(desc.id, s),
                    Err(e) => warn!(storage_id = %desc.id, error = %e, "failed to init storage"),
                }
            }
            info!(count = descs.len(), "storage registry populated");
        }
        Err(e) => warn!(error = %e, "could not load storages"),
    }

    // Vault: Stage 0 — in-memory. Stage 5 — DPAPI + AES-GCM.
    let vault = Arc::new(InMemoryVault::new());

    let onec = Arc::new(DefaultOneCRunner::new(OneCConfig {
        one_cv_8_exe: std::path::PathBuf::from(r"C:\Program Files\1cv8\common\1cestart.exe"),
        rac_exe: None,
    }));

    let notifier = Arc::new(SystemNotifier::new());

    let (events_tx, _) = broadcast::channel(256);

    Ok(AppContext {
        sources,
        jobs,
        storages,
        settings,
        storage_registry: registry,
        vault,
        onec,
        notifier,
        clock: Arc::new(SystemClock),
        events: events_tx,
    })
}

pub async fn read_max_parallel(ctx: &AppContext) -> usize {
    match ctx.settings.get("max_parallel").await {
        Ok(Some(val)) => serde_json::from_str::<usize>(&val).unwrap_or(2).clamp(1, 16),
        _ => 2,
    }
}

pub fn make_storage(desc: &StorageDescriptor) -> anyhow::Result<Arc<dyn domain::Storage>> {
    let cfg: serde_json::Value = serde_json::from_str(&desc.config_json)
        .unwrap_or(serde_json::Value::Object(Default::default()));
    match desc.kind {
        StorageKind::Local => {
            let root = cfg["root"].as_str().unwrap_or(".");
            Ok(Arc::new(LocalStorage::new(root)))
        }
        StorageKind::Smb => {
            let unc = cfg["unc"].as_str().unwrap_or(".");
            Ok(Arc::new(LocalStorage::new(unc)))
        }
        kind => anyhow::bail!("unsupported storage kind for registry: {}", kind.as_str()),
    }
}

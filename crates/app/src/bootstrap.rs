//! Корень dependency injection: собирает `AppContext` из реальных адаптеров.

use std::sync::Arc;

use application::clock::SystemClock;
use application::context::{AppContext, StorageRegistry};
use infra_1c::{DefaultOneCRunner, OneCConfig};
use infra_secrets::InMemoryVault;
use infra_storage_sqlite::{
    SqliteJobRepository, SqliteSourceRepository, SqliteStorageRepository,
};
use tokio::sync::broadcast;
use tracing::info;

use crate::notifications::SystemNotifier;

pub async fn build_context() -> anyhow::Result<AppContext> {
    let db_path = shared::paths::app_db_path();
    info!(db = %db_path.display(), "opening sqlite pool");
    let pool = infra_storage_sqlite::open_app_db(&db_path).await?;

    let sources = Arc::new(SqliteSourceRepository::new(pool.clone()));
    let jobs = Arc::new(SqliteJobRepository::new(pool.clone()));
    let storages = Arc::new(SqliteStorageRepository::new(pool.clone()));

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
        storage_registry: Arc::new(StorageRegistry::new()),
        vault,
        onec,
        notifier,
        clock: Arc::new(SystemClock),
        events: events_tx,
    })
}

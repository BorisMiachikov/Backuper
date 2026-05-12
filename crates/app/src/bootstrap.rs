//! Корень dependency injection: собирает `AppContext` из реальных адаптеров.

use std::sync::Arc;

use application::clock::SystemClock;
use application::context::{AppContext, StorageRegistry};
use domain::{SecretVault, StorageDescriptor, StorageKind, StorageRepository};
use infra_1c::{DefaultOneCRunner, OneCConfig};
use infra_cloud_gdrive::GDriveClient;
use infra_cloud_yadisk::YaDiskClient;
use infra_fs::LocalStorage;
use infra_secrets::DpapiVault;
use infra_storage_sqlite::{
    SqliteJobRepository, SqliteSettingsRepository, SqliteSourceRepository, SqliteStorageRepository,
};
use secrecy::ExposeSecret;
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

    // Vault: DpapiVault (файл + Windows DPAPI).
    let vault_path = shared::paths::data_dir().join("vault.dat");
    let vault: Arc<dyn SecretVault> = Arc::new(DpapiVault::open(vault_path).await?);

    // Заполняем StorageRegistry по записям из БД.
    let registry = Arc::new(StorageRegistry::new());
    match storages.list().await {
        Ok(descs) => {
            for desc in &descs {
                if !desc.enabled {
                    continue;
                }
                match make_storage(desc, &vault).await {
                    Ok(s) => registry.register(desc.id, s),
                    Err(e) => warn!(storage_id = %desc.id, error = %e, "failed to init storage"),
                }
            }
            info!(count = descs.len(), "storage registry populated");
        }
        Err(e) => warn!(error = %e, "could not load storages"),
    }

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

/// Создаёт живой `Storage`-адаптер по дескриптору.
///
/// Для облачных хранилищ сначала пытается взять токен из vault (через `secret_ref`),
/// с откатом на plain-токен в `config_json` (legacy / новое хранилище до первого сохранения).
pub async fn make_storage(
    desc: &StorageDescriptor,
    vault: &Arc<dyn SecretVault>,
) -> anyhow::Result<Arc<dyn domain::Storage>> {
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
        StorageKind::YaDisk => {
            let token = resolve_cloud_token(&cfg, desc.secret_ref.as_deref(), vault).await;
            Ok(Arc::new(YaDiskClient::new(token)))
        }
        StorageKind::GDrive => {
            let token = resolve_cloud_token(&cfg, desc.secret_ref.as_deref(), vault).await;
            Ok(Arc::new(GDriveClient::new(token)))
        }
    }
}

/// Получить OAuth-токен: сначала из vault (если задан `secret_ref`),
/// потом fallback на plain `config_json["token"]`.
async fn resolve_cloud_token(
    cfg: &serde_json::Value,
    secret_ref: Option<&str>,
    vault: &Arc<dyn SecretVault>,
) -> String {
    if let Some(r) = secret_ref {
        match vault.get(r).await {
            Ok(sv) => return sv.payload.expose_secret().to_owned(),
            Err(e) => warn!(r#ref = r, error = %e, "vault lookup failed, falling back to config"),
        }
    }
    cfg["token"].as_str().unwrap_or("").to_owned()
}

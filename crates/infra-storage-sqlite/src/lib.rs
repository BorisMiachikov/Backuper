//! SQLite-инфраструктура: пул соединений + миграции + репозитории.
//!
//! Миграции лежат в `<workspace>/migrations` и компилируются макросом
//! `sqlx::migrate!()`.

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

mod mappers;
pub mod repos;

pub use repos::{SqliteJobRepository, SqliteSourceRepository, SqliteStorageRepository};

/// Открывает пул соединений к `app.db` (создаёт файл при отсутствии)
/// и прогоняет встроенные миграции.
pub async fn open_app_db(path: &Path) -> anyhow::Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect_with(opts)
        .await?;

    sqlx::migrate!("../../migrations").run(&pool).await?;

    Ok(pool)
}

/// Создаёт пул для in-memory БД с прогнанными миграциями. Только для тестов.
#[cfg(any(test, feature = "test-utils"))]
pub async fn open_in_memory() -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::new()
        .in_memory(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;
    Ok(pool)
}

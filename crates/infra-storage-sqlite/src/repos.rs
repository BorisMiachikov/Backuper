//! SQLx-репозитории.
//!
//! Соответствие миграциям: см. `migrations/0001_init.sql`. Любые изменения схемы
//! идут только через новые миграции, не правкой существующих.

use async_trait::async_trait;
use domain::repo::StorageDescriptor;
use domain::{
    DomainError, DomainResult, Job, JobRepository, JobRun, Schedule, Source, SourceKind,
    SourceRepository, StorageRepository,
};
use sqlx::{Row, SqlitePool};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::mappers::source_kind;

fn map_sqlx_err(e: sqlx::Error) -> DomainError {
    DomainError::Repository(e.to_string())
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("rfc3339 format never fails for UTC")
}

fn parse_rfc3339(s: &str) -> DomainResult<OffsetDateTime> {
    OffsetDateTime::parse(s, &Rfc3339)
        .map_err(|e| DomainError::Repository(format!("invalid datetime: {e}")))
}

// ──────────────────────────── Sources ────────────────────────────

#[derive(Clone)]
pub struct SqliteSourceRepository {
    pub pool: SqlitePool,
}

impl SqliteSourceRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SourceRepository for SqliteSourceRepository {
    async fn get(&self, id: Uuid) -> DomainResult<Option<Source>> {
        let id_s = id.to_string();
        let row = sqlx::query(
            "SELECT id, kind, name, path, enabled, description, params_json, created_at, updated_at
             FROM sources WHERE id = ?1",
        )
        .bind(&id_s)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_err)?;

        let Some(row) = row else { return Ok(None) };
        let mut s = row_to_source(&row)?;
        s.tags = load_tags(&self.pool, &id_s).await?;
        Ok(Some(s))
    }

    async fn list(&self) -> DomainResult<Vec<Source>> {
        let rows = sqlx::query(
            "SELECT id, kind, name, path, enabled, description, params_json, created_at, updated_at
             FROM sources ORDER BY name COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_err)?;

        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut s = row_to_source(row)?;
            let id_s: String = row.try_get("id").map_err(map_sqlx_err)?;
            s.tags = load_tags(&self.pool, &id_s).await?;
            out.push(s);
        }
        Ok(out)
    }

    async fn upsert(&self, source: &Source) -> DomainResult<()> {
        source.validate()?;
        let (kind_str, params) = source_kind::split(&source.kind);
        let params_json = serde_json::to_string(&params)
            .map_err(|e| DomainError::Repository(format!("serialize params: {e}")))?;
        let id_s = source.id.to_string();
        let path_s = source.path.to_string_lossy().into_owned();
        let created_at = source.created_at.format(&Rfc3339).map_err(|e| {
            DomainError::Repository(format!("format created_at: {e}"))
        })?;
        let updated_at = now_rfc3339();

        let mut tx = self.pool.begin().await.map_err(map_sqlx_err)?;

        sqlx::query(
            "INSERT INTO sources (id, kind, name, path, enabled, description, params_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                kind        = excluded.kind,
                name        = excluded.name,
                path        = excluded.path,
                enabled     = excluded.enabled,
                description = excluded.description,
                params_json = excluded.params_json,
                updated_at  = excluded.updated_at",
        )
        .bind(&id_s)
        .bind(&kind_str)
        .bind(&source.name)
        .bind(&path_s)
        .bind(i64::from(source.enabled))
        .bind(source.description.as_deref())
        .bind(&params_json)
        .bind(&created_at)
        .bind(&updated_at)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_err)?;

        // Полная пересинхронизация тегов: удаляем все и вставляем актуальный набор.
        sqlx::query("DELETE FROM source_tags WHERE source_id = ?1")
            .bind(&id_s)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_err)?;

        for tag in &source.tags {
            sqlx::query("INSERT OR IGNORE INTO source_tags (source_id, tag) VALUES (?1, ?2)")
                .bind(&id_s)
                .bind(tag)
                .execute(&mut *tx)
                .await
                .map_err(map_sqlx_err)?;
        }

        tx.commit().await.map_err(map_sqlx_err)?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> DomainResult<()> {
        sqlx::query("DELETE FROM sources WHERE id = ?1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        Ok(())
    }
}

fn row_to_source(row: &sqlx::sqlite::SqliteRow) -> DomainResult<Source> {
    let id_s: String = row.try_get("id").map_err(map_sqlx_err)?;
    let kind_s: String = row.try_get("kind").map_err(map_sqlx_err)?;
    let params_json: String = row.try_get("params_json").map_err(map_sqlx_err)?;
    let name: String = row.try_get("name").map_err(map_sqlx_err)?;
    let path_s: String = row.try_get("path").map_err(map_sqlx_err)?;
    let enabled_i: i64 = row.try_get("enabled").map_err(map_sqlx_err)?;
    let description: Option<String> = row.try_get("description").map_err(map_sqlx_err)?;
    let created_s: String = row.try_get("created_at").map_err(map_sqlx_err)?;
    let updated_s: String = row.try_get("updated_at").map_err(map_sqlx_err)?;

    let params: serde_json::Value = serde_json::from_str(&params_json)
        .map_err(|e| DomainError::Repository(format!("parse params_json: {e}")))?;
    let kind: SourceKind = source_kind::assemble(&kind_s, &params)
        .map_err(|e| DomainError::Repository(format!("assemble kind: {e}")))?;

    Ok(Source {
        id: Uuid::parse_str(&id_s)
            .map_err(|e| DomainError::Repository(format!("parse uuid: {e}")))?,
        kind,
        name,
        path: std::path::PathBuf::from(path_s),
        enabled: enabled_i != 0,
        description,
        tags: Vec::new(), // догружаются отдельно
        created_at: parse_rfc3339(&created_s)?,
        updated_at: parse_rfc3339(&updated_s)?,
    })
}

async fn load_tags(pool: &SqlitePool, source_id: &str) -> DomainResult<Vec<String>> {
    let rows = sqlx::query("SELECT tag FROM source_tags WHERE source_id = ?1 ORDER BY tag")
        .bind(source_id)
        .fetch_all(pool)
        .await
        .map_err(map_sqlx_err)?;
    let mut tags = Vec::with_capacity(rows.len());
    for row in &rows {
        tags.push(row.try_get::<String, _>("tag").map_err(map_sqlx_err)?);
    }
    Ok(tags)
}

// ──────────────────────────── Jobs (заглушки до Stage 1.2) ────────────────────────────

#[derive(Clone)]
pub struct SqliteJobRepository {
    pub pool: SqlitePool,
}

impl SqliteJobRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl JobRepository for SqliteJobRepository {
    async fn get(&self, _id: Uuid) -> DomainResult<Option<Job>> {
        Ok(None)
    }
    async fn list(&self) -> DomainResult<Vec<Job>> {
        Ok(Vec::new())
    }
    async fn upsert(&self, _job: &Job) -> DomainResult<()> {
        Err(DomainError::Repository("jobs.upsert: stage 1.2".into()))
    }
    async fn delete(&self, _id: Uuid) -> DomainResult<()> {
        Err(DomainError::Repository("jobs.delete: stage 1.2".into()))
    }
    async fn list_schedules(&self) -> DomainResult<Vec<Schedule>> {
        Ok(Vec::new())
    }
    async fn upsert_schedule(&self, _schedule: &Schedule) -> DomainResult<()> {
        Err(DomainError::Repository("schedules.upsert: stage 1.2".into()))
    }
    async fn insert_run(&self, _run: &JobRun) -> DomainResult<()> {
        Err(DomainError::Repository("runs.insert: stage 1.2".into()))
    }
    async fn update_run(&self, _run: &JobRun) -> DomainResult<()> {
        Err(DomainError::Repository("runs.update: stage 1.2".into()))
    }
    async fn list_runs(&self, _job_id: Uuid, _limit: u32) -> DomainResult<Vec<JobRun>> {
        Ok(Vec::new())
    }
    async fn mark_running_as_interrupted(&self) -> DomainResult<u64> {
        Ok(0)
    }
}

// ──────────────────────────── Storages (заглушки до Stage 1.3) ────────────────────────────

#[derive(Clone)]
pub struct SqliteStorageRepository {
    pub pool: SqlitePool,
}

impl SqliteStorageRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl StorageRepository for SqliteStorageRepository {
    async fn get(&self, _id: Uuid) -> DomainResult<Option<StorageDescriptor>> {
        Ok(None)
    }
    async fn list(&self) -> DomainResult<Vec<StorageDescriptor>> {
        Ok(Vec::new())
    }
    async fn upsert(&self, _desc: &StorageDescriptor) -> DomainResult<()> {
        Err(DomainError::Repository("storages.upsert: stage 1.3".into()))
    }
    async fn delete(&self, _id: Uuid) -> DomainResult<()> {
        Err(DomainError::Repository("storages.delete: stage 1.3".into()))
    }
}


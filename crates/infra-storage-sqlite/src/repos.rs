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

use crate::mappers::{job_run, schedule_kind, source_kind};

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

// ──────────────────────────── Jobs ────────────────────────────

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
    async fn get(&self, id: Uuid) -> DomainResult<Option<Job>> {
        let row = sqlx::query(
            "SELECT id, name, source_id, enabled, archive_cfg, retention_cfg, exclude_json,
                    pre_cmd, post_cmd, priority, created_at, updated_at
             FROM jobs WHERE id = ?1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_err)?;

        match row {
            Some(r) => Ok(Some(row_to_job(&r)?)),
            None => Ok(None),
        }
    }

    async fn list(&self) -> DomainResult<Vec<Job>> {
        let rows = sqlx::query(
            "SELECT id, name, source_id, enabled, archive_cfg, retention_cfg, exclude_json,
                    pre_cmd, post_cmd, priority, created_at, updated_at
             FROM jobs ORDER BY name COLLATE NOCASE",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_err)?;
        rows.iter().map(row_to_job).collect()
    }

    async fn upsert(&self, job: &Job) -> DomainResult<()> {
        let archive_cfg = serde_json::to_string(&job.archive)
            .map_err(|e| DomainError::Repository(format!("serialize archive_cfg: {e}")))?;
        let retention_cfg = serde_json::to_string(&job.retention)
            .map_err(|e| DomainError::Repository(format!("serialize retention_cfg: {e}")))?;
        let exclude_json = serde_json::to_string(&job.exclude)
            .map_err(|e| DomainError::Repository(format!("serialize exclude: {e}")))?;
        let created_at = job
            .created_at
            .format(&Rfc3339)
            .map_err(|e| DomainError::Repository(format!("format created_at: {e}")))?;
        let updated_at = now_rfc3339();

        sqlx::query(
            "INSERT INTO jobs (id, name, source_id, enabled, archive_cfg, retention_cfg,
                               exclude_json, pre_cmd, post_cmd, priority, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
                name          = excluded.name,
                source_id     = excluded.source_id,
                enabled       = excluded.enabled,
                archive_cfg   = excluded.archive_cfg,
                retention_cfg = excluded.retention_cfg,
                exclude_json  = excluded.exclude_json,
                pre_cmd       = excluded.pre_cmd,
                post_cmd      = excluded.post_cmd,
                priority      = excluded.priority,
                updated_at    = excluded.updated_at",
        )
        .bind(job.id.to_string())
        .bind(&job.name)
        .bind(job.source_id.to_string())
        .bind(i64::from(job.enabled))
        .bind(&archive_cfg)
        .bind(&retention_cfg)
        .bind(&exclude_json)
        .bind(job.pre_cmd.as_deref())
        .bind(job.post_cmd.as_deref())
        .bind(i64::from(job.priority))
        .bind(&created_at)
        .bind(&updated_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_err)?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> DomainResult<()> {
        sqlx::query("DELETE FROM jobs WHERE id = ?1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        Ok(())
    }

    async fn list_schedules(&self) -> DomainResult<Vec<Schedule>> {
        let rows = sqlx::query(
            "SELECT id, job_id, kind, cron_expr, run_at, next_fire, enabled FROM schedules",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_err)?;
        rows.iter().map(row_to_schedule).collect()
    }

    async fn upsert_schedule(&self, schedule: &Schedule) -> DomainResult<()> {
        let cols = schedule_kind::split(&schedule.kind);
        let next_fire = schedule
            .next_fire
            .map(|t| t.format(&Rfc3339))
            .transpose()
            .map_err(|e| DomainError::Repository(format!("format next_fire: {e}")))?;
        sqlx::query(
            "INSERT INTO schedules (id, job_id, kind, cron_expr, run_at, next_fire, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                job_id    = excluded.job_id,
                kind      = excluded.kind,
                cron_expr = excluded.cron_expr,
                run_at    = excluded.run_at,
                next_fire = excluded.next_fire,
                enabled   = excluded.enabled",
        )
        .bind(schedule.id.to_string())
        .bind(schedule.job_id.to_string())
        .bind(&cols.kind)
        .bind(cols.cron_expr)
        .bind(cols.run_at)
        .bind(next_fire)
        .bind(i64::from(schedule.enabled))
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_err)?;
        Ok(())
    }

    async fn insert_run(&self, run: &JobRun) -> DomainResult<()> {
        let started = run
            .started_at
            .format(&Rfc3339)
            .map_err(|e| DomainError::Repository(format!("format started_at: {e}")))?;
        let finished = run
            .finished_at
            .map(|t| t.format(&Rfc3339))
            .transpose()
            .map_err(|e| DomainError::Repository(format!("format finished_at: {e}")))?;
        sqlx::query(
            "INSERT INTO job_runs (id, job_id, trigger, started_at, finished_at, status,
                                   bytes_in, bytes_out, files_count, archive_path,
                                   error_msg, host, attempt)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )
        .bind(run.id.to_string())
        .bind(run.job_id.to_string())
        .bind(job_run::trigger_to_str(run.trigger))
        .bind(&started)
        .bind(finished)
        .bind(job_run::status_to_str(run.status))
        .bind(i64::try_from(run.bytes_in).unwrap_or(i64::MAX))
        .bind(i64::try_from(run.bytes_out).unwrap_or(i64::MAX))
        .bind(i64::try_from(run.files_count).unwrap_or(i64::MAX))
        .bind(run.archive_path.as_deref())
        .bind(run.error_msg.as_deref())
        .bind(&run.host)
        .bind(i64::from(run.attempt))
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_err)?;
        Ok(())
    }

    async fn update_run(&self, run: &JobRun) -> DomainResult<()> {
        let finished = run
            .finished_at
            .map(|t| t.format(&Rfc3339))
            .transpose()
            .map_err(|e| DomainError::Repository(format!("format finished_at: {e}")))?;
        sqlx::query(
            "UPDATE job_runs SET
                finished_at  = ?1,
                status       = ?2,
                bytes_in     = ?3,
                bytes_out    = ?4,
                files_count  = ?5,
                archive_path = ?6,
                error_msg    = ?7,
                attempt      = ?8
             WHERE id = ?9",
        )
        .bind(finished)
        .bind(job_run::status_to_str(run.status))
        .bind(i64::try_from(run.bytes_in).unwrap_or(i64::MAX))
        .bind(i64::try_from(run.bytes_out).unwrap_or(i64::MAX))
        .bind(i64::try_from(run.files_count).unwrap_or(i64::MAX))
        .bind(run.archive_path.as_deref())
        .bind(run.error_msg.as_deref())
        .bind(i64::from(run.attempt))
        .bind(run.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_err)?;
        Ok(())
    }

    async fn list_runs(&self, job_id: Uuid, limit: u32) -> DomainResult<Vec<JobRun>> {
        let rows = sqlx::query(
            "SELECT id, job_id, trigger, started_at, finished_at, status,
                    bytes_in, bytes_out, files_count, archive_path, error_msg, host, attempt
             FROM job_runs WHERE job_id = ?1 ORDER BY started_at DESC LIMIT ?2",
        )
        .bind(job_id.to_string())
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_err)?;
        rows.iter().map(row_to_job_run).collect()
    }

    async fn mark_running_as_interrupted(&self) -> DomainResult<u64> {
        let res = sqlx::query(
            "UPDATE job_runs SET status = ?1, finished_at = ?2
             WHERE status IN ('running', 'pending')",
        )
        .bind(job_run::status_to_str(domain::JobRunStatus::Interrupted))
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_err)?;
        Ok(res.rows_affected())
    }
}

fn row_to_job(row: &sqlx::sqlite::SqliteRow) -> DomainResult<Job> {
    let id_s: String = row.try_get("id").map_err(map_sqlx_err)?;
    let source_id_s: String = row.try_get("source_id").map_err(map_sqlx_err)?;
    let name: String = row.try_get("name").map_err(map_sqlx_err)?;
    let enabled_i: i64 = row.try_get("enabled").map_err(map_sqlx_err)?;
    let archive_s: String = row.try_get("archive_cfg").map_err(map_sqlx_err)?;
    let retention_s: String = row.try_get("retention_cfg").map_err(map_sqlx_err)?;
    let exclude_s: String = row.try_get("exclude_json").map_err(map_sqlx_err)?;
    let pre_cmd: Option<String> = row.try_get("pre_cmd").map_err(map_sqlx_err)?;
    let post_cmd: Option<String> = row.try_get("post_cmd").map_err(map_sqlx_err)?;
    let priority_i: i64 = row.try_get("priority").map_err(map_sqlx_err)?;
    let created_s: String = row.try_get("created_at").map_err(map_sqlx_err)?;
    let updated_s: String = row.try_get("updated_at").map_err(map_sqlx_err)?;

    Ok(Job {
        id: Uuid::parse_str(&id_s)
            .map_err(|e| DomainError::Repository(format!("parse uuid: {e}")))?,
        source_id: Uuid::parse_str(&source_id_s)
            .map_err(|e| DomainError::Repository(format!("parse source uuid: {e}")))?,
        name,
        enabled: enabled_i != 0,
        archive: serde_json::from_str(&archive_s)
            .map_err(|e| DomainError::Repository(format!("parse archive_cfg: {e}")))?,
        retention: serde_json::from_str(&retention_s)
            .map_err(|e| DomainError::Repository(format!("parse retention_cfg: {e}")))?,
        exclude: serde_json::from_str(&exclude_s)
            .map_err(|e| DomainError::Repository(format!("parse exclude: {e}")))?,
        pre_cmd,
        post_cmd,
        priority: i16::try_from(priority_i).unwrap_or(0),
        targets: Vec::new(),
        created_at: parse_rfc3339(&created_s)?,
        updated_at: parse_rfc3339(&updated_s)?,
    })
}

fn row_to_schedule(row: &sqlx::sqlite::SqliteRow) -> DomainResult<Schedule> {
    let id_s: String = row.try_get("id").map_err(map_sqlx_err)?;
    let job_id_s: String = row.try_get("job_id").map_err(map_sqlx_err)?;
    let kind_s: String = row.try_get("kind").map_err(map_sqlx_err)?;
    let cron_expr: Option<String> = row.try_get("cron_expr").map_err(map_sqlx_err)?;
    let run_at: Option<String> = row.try_get("run_at").map_err(map_sqlx_err)?;
    let next_fire_s: Option<String> = row.try_get("next_fire").map_err(map_sqlx_err)?;
    let enabled_i: i64 = row.try_get("enabled").map_err(map_sqlx_err)?;

    let kind = schedule_kind::assemble(&kind_s, cron_expr.as_deref(), run_at.as_deref())
        .map_err(|e| DomainError::Repository(format!("assemble schedule: {e}")))?;
    let next_fire = next_fire_s.as_deref().map(parse_rfc3339).transpose()?;

    Ok(Schedule {
        id: Uuid::parse_str(&id_s)
            .map_err(|e| DomainError::Repository(format!("parse schedule uuid: {e}")))?,
        job_id: Uuid::parse_str(&job_id_s)
            .map_err(|e| DomainError::Repository(format!("parse job uuid: {e}")))?,
        kind,
        enabled: enabled_i != 0,
        next_fire,
    })
}

fn row_to_job_run(row: &sqlx::sqlite::SqliteRow) -> DomainResult<JobRun> {
    let id_s: String = row.try_get("id").map_err(map_sqlx_err)?;
    let job_id_s: String = row.try_get("job_id").map_err(map_sqlx_err)?;
    let trigger_s: String = row.try_get("trigger").map_err(map_sqlx_err)?;
    let status_s: String = row.try_get("status").map_err(map_sqlx_err)?;
    let started_s: String = row.try_get("started_at").map_err(map_sqlx_err)?;
    let finished_s: Option<String> = row.try_get("finished_at").map_err(map_sqlx_err)?;
    let bytes_in: i64 = row.try_get("bytes_in").map_err(map_sqlx_err)?;
    let bytes_out: i64 = row.try_get("bytes_out").map_err(map_sqlx_err)?;
    let files_count: i64 = row.try_get("files_count").map_err(map_sqlx_err)?;
    let archive_path: Option<String> = row.try_get("archive_path").map_err(map_sqlx_err)?;
    let error_msg: Option<String> = row.try_get("error_msg").map_err(map_sqlx_err)?;
    let host: String = row.try_get("host").map_err(map_sqlx_err)?;
    let attempt_i: i64 = row.try_get("attempt").map_err(map_sqlx_err)?;

    Ok(JobRun {
        id: Uuid::parse_str(&id_s)
            .map_err(|e| DomainError::Repository(format!("parse uuid: {e}")))?,
        job_id: Uuid::parse_str(&job_id_s)
            .map_err(|e| DomainError::Repository(format!("parse job uuid: {e}")))?,
        trigger: job_run::trigger_from_str(&trigger_s)
            .map_err(|e| DomainError::Repository(e.to_string()))?,
        status: job_run::status_from_str(&status_s)
            .map_err(|e| DomainError::Repository(e.to_string()))?,
        started_at: parse_rfc3339(&started_s)?,
        finished_at: finished_s.as_deref().map(parse_rfc3339).transpose()?,
        bytes_in: u64::try_from(bytes_in).unwrap_or(0),
        bytes_out: u64::try_from(bytes_out).unwrap_or(0),
        files_count: u64::try_from(files_count).unwrap_or(0),
        archive_path,
        error_msg,
        host,
        attempt: u32::try_from(attempt_i).unwrap_or(0),
    })
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


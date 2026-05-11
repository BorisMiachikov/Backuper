//! Реализации репозиториев поверх SQLx.
//!
//! Stage 0 — заглушки с возвратом пустых списков / `NotFound`. Реальные SQL-запросы
//! добавляются на Stage 1 (Foundation), когда выровняем схему миграций.

use async_trait::async_trait;
use domain::{
    DomainError, DomainResult, Job, JobRepository, JobRun, Schedule, Source, SourceRepository,
    StorageRepository,
};
use domain::repo::StorageDescriptor;
use sqlx::SqlitePool;
use uuid::Uuid;

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
    async fn get(&self, _id: Uuid) -> DomainResult<Option<Source>> {
        // TODO Stage 1
        Ok(None)
    }
    async fn list(&self) -> DomainResult<Vec<Source>> {
        Ok(Vec::new())
    }
    async fn upsert(&self, _source: &Source) -> DomainResult<()> {
        Err(DomainError::Repository("upsert: not implemented".into()))
    }
    async fn delete(&self, _id: Uuid) -> DomainResult<()> {
        Err(DomainError::Repository("delete: not implemented".into()))
    }
}

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
        Err(DomainError::Repository("upsert: not implemented".into()))
    }
    async fn delete(&self, _id: Uuid) -> DomainResult<()> {
        Err(DomainError::Repository("delete: not implemented".into()))
    }
    async fn list_schedules(&self) -> DomainResult<Vec<Schedule>> {
        Ok(Vec::new())
    }
    async fn upsert_schedule(&self, _schedule: &Schedule) -> DomainResult<()> {
        Err(DomainError::Repository("upsert_schedule: not implemented".into()))
    }
    async fn insert_run(&self, _run: &JobRun) -> DomainResult<()> {
        Err(DomainError::Repository("insert_run: not implemented".into()))
    }
    async fn update_run(&self, _run: &JobRun) -> DomainResult<()> {
        Err(DomainError::Repository("update_run: not implemented".into()))
    }
    async fn list_runs(&self, _job_id: Uuid, _limit: u32) -> DomainResult<Vec<JobRun>> {
        Ok(Vec::new())
    }
    async fn mark_running_as_interrupted(&self) -> DomainResult<u64> {
        Ok(0)
    }
}

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
        Err(DomainError::Repository("upsert: not implemented".into()))
    }
    async fn delete(&self, _id: Uuid) -> DomainResult<()> {
        Err(DomainError::Repository("delete: not implemented".into()))
    }
}

use async_trait::async_trait;
use uuid::Uuid;

use crate::errors::DomainResult;
use crate::job::{Job, JobRun, JobTarget};
use crate::schedule::Schedule;
use crate::source::Source;

#[derive(Debug, Clone)]
pub struct StorageDescriptor {
    pub id: Uuid,
    pub kind: crate::storage::StorageKind,
    pub name: String,
    pub config_json: String,
    pub secret_ref: Option<String>,
    pub enabled: bool,
}

#[async_trait]
pub trait SourceRepository: Send + Sync {
    async fn get(&self, id: Uuid) -> DomainResult<Option<Source>>;
    async fn list(&self) -> DomainResult<Vec<Source>>;
    async fn upsert(&self, source: &Source) -> DomainResult<()>;
    async fn delete(&self, id: Uuid) -> DomainResult<()>;
}

#[async_trait]
pub trait JobRepository: Send + Sync {
    async fn get(&self, id: Uuid) -> DomainResult<Option<Job>>;
    async fn list(&self) -> DomainResult<Vec<Job>>;
    async fn upsert(&self, job: &Job) -> DomainResult<()>;
    async fn delete(&self, id: Uuid) -> DomainResult<()>;

    async fn list_schedules(&self) -> DomainResult<Vec<Schedule>>;
    async fn upsert_schedule(&self, schedule: &Schedule) -> DomainResult<()>;

    async fn insert_run(&self, run: &JobRun) -> DomainResult<()>;
    async fn update_run(&self, run: &JobRun) -> DomainResult<()>;
    async fn list_runs(&self, job_id: Uuid, limit: u32) -> DomainResult<Vec<JobRun>>;
    async fn list_all_runs(&self, limit: u32) -> DomainResult<Vec<JobRun>>;
    async fn mark_running_as_interrupted(&self) -> DomainResult<u64>;

    async fn upsert_job_targets(&self, job_id: Uuid, targets: &[JobTarget]) -> DomainResult<()>;
}

#[async_trait]
pub trait SettingsRepository: Send + Sync {
    async fn get(&self, key: &str) -> DomainResult<Option<String>>;
    async fn set(&self, key: &str, value_json: &str) -> DomainResult<()>;
}

#[async_trait]
pub trait StorageRepository: Send + Sync {
    async fn get(&self, id: Uuid) -> DomainResult<Option<StorageDescriptor>>;
    async fn list(&self) -> DomainResult<Vec<StorageDescriptor>>;
    async fn upsert(&self, desc: &StorageDescriptor) -> DomainResult<()>;
    async fn delete(&self, id: Uuid) -> DomainResult<()>;
}

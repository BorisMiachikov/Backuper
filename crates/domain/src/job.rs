use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::archive::{ArchiveConfig, ExcludeRules, RetentionPolicy};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobTarget {
    pub storage_id: Uuid,
    pub remote_path: String,
    pub order_idx: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: Uuid,
    pub source_id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub archive: ArchiveConfig,
    pub retention: RetentionPolicy,
    pub exclude: ExcludeRules,
    pub pre_cmd: Option<String>,
    pub post_cmd: Option<String>,
    pub priority: i16,
    pub targets: Vec<JobTarget>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobRunStatus {
    Pending,
    Running,
    Success,
    Failed,
    Cancelled,
    Skipped,
    Interrupted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunTrigger {
    Scheduled,
    Manual,
    Retry,
}

#[derive(Debug, Clone)]
pub struct JobTrigger {
    pub run_id: Uuid,
    pub job_id: Uuid,
    pub trigger: RunTrigger,
    pub attempt: u32,
}

impl JobTrigger {
    pub fn manual(job_id: Uuid) -> Self {
        Self {
            run_id: Uuid::now_v7(),
            job_id,
            trigger: RunTrigger::Manual,
            attempt: 0,
        }
    }

    pub fn scheduled(job_id: Uuid) -> Self {
        Self {
            run_id: Uuid::now_v7(),
            job_id,
            trigger: RunTrigger::Scheduled,
            attempt: 0,
        }
    }

    pub fn retry_of(other: &Self) -> Self {
        Self {
            run_id: Uuid::now_v7(),
            job_id: other.job_id,
            trigger: RunTrigger::Retry,
            attempt: other.attempt + 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRun {
    pub id: Uuid,
    pub job_id: Uuid,
    pub trigger: RunTrigger,
    pub status: JobRunStatus,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub finished_at: Option<OffsetDateTime>,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub files_count: u64,
    pub archive_path: Option<String>,
    pub error_msg: Option<String>,
    pub host: String,
    pub attempt: u32,
}

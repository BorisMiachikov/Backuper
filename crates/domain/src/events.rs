use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::job::JobRunStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRunStarted {
    pub run_id: Uuid,
    pub job_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRunFinished {
    pub run_id: Uuid,
    pub job_id: Uuid,
    pub status: JobRunStatus,
    pub bytes_out: u64,
    pub duration_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    Prepare,
    PreHook,
    AcquireLock,
    Collect,
    Archive,
    Verify,
    Upload,
    Retention,
    PostHook,
    Cleanup,
    Persist,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageProgress {
    pub run_id: Uuid,
    pub job_id: Uuid,
    pub stage: PipelineStage,
    pub percent: f32,
    pub bytes_done: u64,
    pub bytes_total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DomainEvent {
    JobRunStarted(JobRunStarted),
    JobRunFinished(JobRunFinished),
    StageProgress(StageProgress),
    SourceChanged { source_id: Uuid },
    JobChanged { job_id: Uuid },
    StorageChanged { storage_id: Uuid },
    NeedsReauth { storage_id: Uuid },
}

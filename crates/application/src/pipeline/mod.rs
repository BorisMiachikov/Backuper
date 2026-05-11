//! Стадии бэкапа: Prepare → PreHook → AcquireLock → Collect → Archive
//! → Verify → Upload → Retention → PostHook → Cleanup → Persist.
//!
//! На Stage 0 — только каркас, реальные стадии будут добавлены на Stage 2-3.

use domain::{Job, JobTrigger};
use thiserror::Error;

use crate::context::AppContext;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("not implemented yet (stage 0 stub)")]
    NotImplemented,
    #[error("retryable: {0}")]
    Retryable(String),
    #[error("fatal: {0}")]
    Fatal(String),
}

impl PipelineError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Retryable(_))
    }
}

pub async fn run(
    _ctx: &AppContext,
    _job: &Job,
    _trigger: &JobTrigger,
) -> Result<(), PipelineError> {
    // TODO Stage 2: реальные стадии.
    Err(PipelineError::NotImplemented)
}

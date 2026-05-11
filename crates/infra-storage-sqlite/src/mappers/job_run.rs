use domain::{JobRunStatus, RunTrigger};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("unknown trigger discriminator: {0}")]
    UnknownTrigger(String),
    #[error("unknown status discriminator: {0}")]
    UnknownStatus(String),
}

pub fn trigger_to_str(t: RunTrigger) -> &'static str {
    match t {
        RunTrigger::Scheduled => "scheduled",
        RunTrigger::Manual => "manual",
        RunTrigger::Retry => "retry",
    }
}

pub fn trigger_from_str(s: &str) -> Result<RunTrigger, ParseError> {
    match s {
        "scheduled" => Ok(RunTrigger::Scheduled),
        "manual" => Ok(RunTrigger::Manual),
        "retry" => Ok(RunTrigger::Retry),
        other => Err(ParseError::UnknownTrigger(other.into())),
    }
}

pub fn status_to_str(s: JobRunStatus) -> &'static str {
    match s {
        JobRunStatus::Pending => "pending",
        JobRunStatus::Running => "running",
        JobRunStatus::Success => "success",
        JobRunStatus::Failed => "failed",
        JobRunStatus::Cancelled => "cancelled",
        JobRunStatus::Skipped => "skipped",
        JobRunStatus::Interrupted => "interrupted",
    }
}

pub fn status_from_str(s: &str) -> Result<JobRunStatus, ParseError> {
    match s {
        "pending" => Ok(JobRunStatus::Pending),
        "running" => Ok(JobRunStatus::Running),
        "success" => Ok(JobRunStatus::Success),
        "failed" => Ok(JobRunStatus::Failed),
        "cancelled" => Ok(JobRunStatus::Cancelled),
        "skipped" => Ok(JobRunStatus::Skipped),
        "interrupted" => Ok(JobRunStatus::Interrupted),
        other => Err(ParseError::UnknownStatus(other.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_roundtrip() {
        for t in [RunTrigger::Scheduled, RunTrigger::Manual, RunTrigger::Retry] {
            assert_eq!(trigger_from_str(trigger_to_str(t)).unwrap(), t);
        }
    }

    #[test]
    fn status_roundtrip() {
        for s in [
            JobRunStatus::Pending,
            JobRunStatus::Running,
            JobRunStatus::Success,
            JobRunStatus::Failed,
            JobRunStatus::Cancelled,
            JobRunStatus::Skipped,
            JobRunStatus::Interrupted,
        ] {
            assert_eq!(status_from_str(status_to_str(s)).unwrap(), s);
        }
    }
}

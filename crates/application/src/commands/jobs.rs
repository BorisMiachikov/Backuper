use std::sync::Arc;

use domain::{DomainResult, Job, JobRun, JobTrigger};
use uuid::Uuid;

use crate::context::AppContext;
use crate::scheduler::Scheduler;

pub async fn list(ctx: &AppContext) -> DomainResult<Vec<Job>> {
    ctx.jobs.list().await
}

pub async fn upsert(ctx: &AppContext, job: &Job) -> DomainResult<()> {
    ctx.jobs.upsert(job).await?;
    let _ = ctx.events.send(domain::DomainEvent::JobChanged { job_id: job.id });
    Ok(())
}

pub async fn delete(ctx: &AppContext, id: Uuid) -> DomainResult<()> {
    ctx.jobs.delete(id).await?;
    let _ = ctx.events.send(domain::DomainEvent::JobChanged { job_id: id });
    Ok(())
}

pub async fn run_now(scheduler: &Arc<Scheduler>, job_id: Uuid) {
    scheduler.enqueue(JobTrigger::manual(job_id)).await;
}

pub async fn list_runs(ctx: &AppContext, job_id: Uuid, limit: u32) -> DomainResult<Vec<JobRun>> {
    ctx.jobs.list_runs(job_id, limit).await
}

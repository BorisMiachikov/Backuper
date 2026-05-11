use std::sync::Arc;

use domain::JobTrigger;
use tracing::{debug, info, warn};

use super::Scheduler;

pub(super) async fn dispatch_loop(sched: Arc<Scheduler>) {
    let rx = sched.queue.receiver();
    loop {
        tokio::select! {
            _ = sched.cancel.cancelled() => {
                info!("scheduler: cancelled, stopping worker dispatch");
                break;
            }
            trigger = rx.recv() => {
                match trigger {
                    Ok(t) => spawn_one(&sched, t).await,
                    Err(_closed) => {
                        warn!("scheduler: queue channel closed");
                        break;
                    }
                }
            }
        }
    }
}

async fn spawn_one(sched: &Arc<Scheduler>, trigger: JobTrigger) {
    let permit = match sched.permits.clone().acquire_owned().await {
        Ok(p) => p,
        Err(_) => {
            warn!("scheduler: semaphore closed");
            return;
        }
    };
    let sched = sched.clone();
    tokio::spawn(async move {
        let _permit = permit;
        run_one(&sched, trigger).await;
    });
}

async fn run_one(sched: &Arc<Scheduler>, trigger: JobTrigger) {
    let Ok(Some(job)) = sched.ctx.jobs.get(trigger.job_id).await else {
        warn!(job_id = %trigger.job_id, "scheduler: job not found");
        return;
    };

    // Conflict-guard: одна и та же source не запускается параллельно.
    if sched
        .conflict
        .insert(job.source_id, trigger.run_id)
        .is_some()
    {
        info!(
            job_id = %job.id,
            source_id = %job.source_id,
            "scheduler: skipped, source busy"
        );
        return;
    }

    debug!(
        job_id = %job.id,
        run_id = %trigger.run_id,
        attempt = trigger.attempt,
        "scheduler: starting pipeline"
    );

    // Pipeline пока заглушка — будет в Stage 2.
    let res = crate::pipeline::run(&sched.ctx, &job, &trigger).await;

    sched.conflict.remove(&job.source_id);

    if let Err(e) = res {
        warn!(job_id = %job.id, error = %e, "scheduler: job failed");
        // Retry-логику включим после реальных стадий pipeline.
    }
}

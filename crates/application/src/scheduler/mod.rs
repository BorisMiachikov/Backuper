//! Планировщик задач: очередь, worker pool, conflict-map, retry.

mod queue;
mod retry;
mod worker;

use std::str::FromStr;
use std::sync::Arc;

use dashmap::DashMap;
use domain::{JobTrigger, ScheduleKind};
use time::OffsetDateTime;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use uuid::Uuid;

use crate::context::AppContext;

pub use queue::JobQueue;
pub use retry::RetryPolicy;

pub struct Scheduler {
    pub(crate) ctx: AppContext,
    pub(crate) permits: Arc<Semaphore>,
    pub(crate) conflict: Arc<DashMap<Uuid, Uuid>>,
    pub(crate) queue: Arc<JobQueue>,
    pub(crate) cancel: CancellationToken,
    pub(crate) retry: RetryPolicy,
}

impl Scheduler {
    pub fn new(ctx: AppContext, max_parallel: usize) -> Self {
        Self {
            ctx,
            permits: Arc::new(Semaphore::new(max_parallel.max(1))),
            conflict: Arc::new(DashMap::new()),
            queue: Arc::new(JobQueue::new()),
            cancel: CancellationToken::new(),
            retry: RetryPolicy::default(),
        }
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Поставить задачу в очередь.
    pub async fn enqueue(&self, trigger: JobTrigger) {
        self.queue.push(trigger).await;
    }

    /// Главный loop: ticker для расписаний + диспатч worker'ов.
    pub async fn run(self: Arc<Self>) {
        let ticker = self.clone();
        tokio::spawn(async move { ticker.ticker_loop().await; });

        let worker_loop = self.clone();
        worker::dispatch_loop(worker_loop).await;
    }

    async fn ticker_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => break,
                _ = interval.tick() => self.poll_schedules().await,
            }
        }
    }

    async fn poll_schedules(&self) {
        let now = OffsetDateTime::now_utc();
        let schedules = match self.ctx.jobs.list_schedules().await {
            Ok(s) => s,
            Err(e) => { warn!(error = %e, "poll_schedules: list failed"); return; }
        };

        for mut sched in schedules {
            if sched.next_fire.is_none() {
                // Первый запуск — вычислить и сохранить next_fire.
                sched.next_fire = next_fire_after(&sched.kind, now);
                let _ = self.ctx.jobs.upsert_schedule(&sched).await;
                continue;
            }
            let fire = sched.next_fire.unwrap();
            if fire > now {
                continue; // ещё не пора
            }
            // Время пришло — ставим в очередь.
            self.queue.push(JobTrigger::scheduled(sched.job_id)).await;
            // Вычисляем следующий запуск.
            sched.next_fire = next_fire_after(&sched.kind, now);
            if let Err(e) = self.ctx.jobs.upsert_schedule(&sched).await {
                warn!(error = %e, schedule_id = %sched.id, "poll_schedules: update next_fire failed");
            }
        }
    }
}

fn next_fire_after(kind: &ScheduleKind, after: OffsetDateTime) -> Option<OffsetDateTime> {
    match kind {
        ScheduleKind::EveryMinutes { minutes } => {
            Some(after + time::Duration::minutes(i64::from(*minutes)))
        }
        ScheduleKind::Daily { hour, minute } => {
            let t = time::Time::from_hms(*hour, *minute, 0).ok()?;
            let today_at = after.replace_time(t);
            if today_at > after {
                Some(today_at)
            } else {
                Some(today_at + time::Duration::days(1))
            }
        }
        ScheduleKind::Weekly { weekday, hour, minute } => {
            let t = time::Time::from_hms(*hour, *minute, 0).ok()?;
            let target_wd = *weekday as i64; // 0=Sun..6=Sat
            let current_wd = after.weekday().number_days_from_sunday() as i64;
            let mut days_ahead = (target_wd - current_wd).rem_euclid(7);
            let candidate = after.replace_time(t) + time::Duration::days(days_ahead);
            if candidate <= after {
                days_ahead += 7;
            }
            Some(after.replace_time(t) + time::Duration::days(days_ahead))
        }
        ScheduleKind::Cron { expression } => {
            let sched = cron::Schedule::from_str(expression).ok()?;
            let after_chrono = chrono::DateTime::<chrono::Utc>::from_timestamp(
                after.unix_timestamp(),
                after.nanosecond(),
            )?;
            let next_chrono = sched.after(&after_chrono).next()?;
            OffsetDateTime::from_unix_timestamp(next_chrono.timestamp()).ok()
        }
    }
}

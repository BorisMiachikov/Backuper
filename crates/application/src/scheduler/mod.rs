//! Планировщик задач: очередь, worker pool, conflict-map, retry.

mod queue;
mod retry;
mod worker;

use std::sync::Arc;

use dashmap::DashMap;
use domain::JobTrigger;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
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
        // Stage 0: пока заглушка. Real impl читает schedules.next_fire <= now.
        // Будет реализовано на Stage 1 (Foundation).
    }
}

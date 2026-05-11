use async_channel::{Receiver, Sender};
use domain::JobTrigger;

/// FIFO-очередь поверх `async_channel`.
///
/// Stage 0 — простая. На Stage 1 заменим на приоритетную (heap по `priority`
/// + `Notify` для пробуждения), сохранив текущий API.
pub struct JobQueue {
    tx: Sender<JobTrigger>,
    rx: Receiver<JobTrigger>,
}

impl JobQueue {
    pub fn new() -> Self {
        let (tx, rx) = async_channel::unbounded();
        Self { tx, rx }
    }

    pub async fn push(&self, trigger: JobTrigger) {
        let _ = self.tx.send(trigger).await;
    }

    pub fn receiver(&self) -> Receiver<JobTrigger> {
        self.rx.clone()
    }
}

impl Default for JobQueue {
    fn default() -> Self {
        Self::new()
    }
}

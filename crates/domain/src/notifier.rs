use async_trait::async_trait;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub title: String,
    pub body: String,
    pub level: NotificationLevel,
}

#[derive(Debug, thiserror::Error)]
pub enum NotifierError {
    #[error("notifier backend unavailable")]
    Unavailable,
    #[error("backend error: {0}")]
    Backend(String),
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, notification: Notification) -> Result<(), NotifierError>;
}

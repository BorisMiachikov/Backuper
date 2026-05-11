//! Notifier-адаптер поверх `notify-rust` (Windows: toast через WinRT, Linux: D-Bus).

use async_trait::async_trait;
use domain::{Notification, NotificationLevel, Notifier, NotifierError};

#[derive(Debug, Default)]
pub struct SystemNotifier;

impl SystemNotifier {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Notifier for SystemNotifier {
    async fn notify(&self, n: Notification) -> Result<(), NotifierError> {
        let mut builder = notify_rust::Notification::new();
        builder.summary(&n.title).body(&n.body).appname("Backuper");
        match n.level {
            NotificationLevel::Info | NotificationLevel::Success => {}
            NotificationLevel::Warning | NotificationLevel::Error => {
                builder.urgency(notify_rust::Urgency::Critical);
            }
        }
        builder
            .show()
            .map(|_| ())
            .map_err(|e| NotifierError::Backend(e.to_string()))
    }
}

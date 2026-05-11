//! Domain layer — чистые типы и трейты-порты.
//!
//! Domain не зависит ни от какой инфраструктуры (БД, FS, HTTP).
//! Все внешние взаимодействия описаны трейтами в модулях [`storage`], [`archive`],
//! [`vault`], [`onec`], [`repo`] и [`notifier`].

pub mod archive;
pub mod errors;
pub mod events;
pub mod job;
pub mod notifier;
pub mod onec;
pub mod repo;
pub mod schedule;
pub mod source;
pub mod storage;
pub mod vault;

pub use archive::{ArchiveConfig, ArchiveFormat, ExcludeRules, RetentionPolicy};
pub use errors::{DomainError, DomainResult};
pub use events::{DomainEvent, JobRunFinished, JobRunStarted, PipelineStage, StageProgress};
pub use job::{Job, JobRun, JobRunStatus, JobTarget, JobTrigger, RunTrigger};
pub use notifier::{Notification, NotificationLevel, Notifier, NotifierError};
pub use onec::OneCRunner;
pub use repo::{JobRepository, SettingsRepository, SourceRepository, StorageDescriptor, StorageRepository};
pub use schedule::{Schedule, ScheduleKind};
pub use source::{Source, SourceKind, SourceTag};
pub use storage::{RemoteEntry, Storage, StorageError, StorageKind, UploadProgress};
pub use vault::{SecretRef, SecretValue, SecretVault};

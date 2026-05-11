//! Application layer — оркестрация, use-cases, планировщик.

pub mod clock;
pub mod commands;
pub mod context;
pub mod pipeline;
pub mod scheduler;

pub use clock::{Clock, SystemClock};
pub use context::AppContext;
pub use scheduler::Scheduler;

pub use domain::{DomainEvent, JobTrigger};

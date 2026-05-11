//! Wiring callbacks from Slint UI to Application use-cases.

use std::sync::Arc;

use application::{AppContext, Scheduler};

use crate::AppWindow;

pub mod jobs;
pub mod journal;
pub mod settings;
pub mod sources;
pub mod storages;

pub fn wire_all(window: &AppWindow, ctx: AppContext, scheduler: Arc<Scheduler>) {
    let ctx = Arc::new(ctx);
    jobs::wire(window, ctx.clone(), scheduler);
    sources::wire(window, ctx.clone());
    storages::wire(window, ctx.clone());
    journal::wire(window, ctx.clone());
    settings::wire(window, ctx);
}

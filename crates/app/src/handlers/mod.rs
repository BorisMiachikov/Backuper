//! Wiring callbacks from Slint UI to Application use-cases.

use std::sync::Arc;

use application::{AppContext, Scheduler};

use crate::AppWindow;

pub mod jobs;

pub fn wire_all(window: &AppWindow, ctx: AppContext, scheduler: Arc<Scheduler>) {
    let ctx = Arc::new(ctx);
    jobs::wire(window, ctx, scheduler);
}

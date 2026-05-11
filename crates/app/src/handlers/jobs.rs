use std::rc::Rc;
use std::sync::Arc;

use application::{AppContext, Scheduler};
use domain::JobTrigger;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use tracing::{info, warn};

use crate::bindings::jobs::job_to_row;
use crate::AppWindow;

pub fn wire(window: &AppWindow, ctx: Arc<AppContext>, sched: Arc<Scheduler>) {
    // Initial empty model — заменим первым refresh'ем.
    window.set_jobs(ModelRc::new(VecModel::<crate::JobRow>::default()));

    // Стартовая загрузка списка заданий.
    refresh_jobs(window, ctx.clone());

    // Callback: «Выполнить сейчас».
    {
        let sched = sched.clone();
        window.on_run_job_now(move |id: SharedString| {
            let Ok(uuid) = uuid::Uuid::parse_str(id.as_str()) else {
                warn!(id = %id, "run_job_now: invalid uuid");
                return;
            };
            info!(job_id = %uuid, "run_job_now requested");
            let sched = sched.clone();
            tokio::spawn(async move {
                sched.enqueue(JobTrigger::manual(uuid)).await;
            });
        });
    }

    // Остальные callbacks — заглушки до Stage 2.
    window.on_edit_job(|id| info!(?id, "edit_job (not implemented)"));
    window.on_toggle_job(|id| info!(?id, "toggle_job (not implemented)"));
    window.on_delete_job(|id| info!(?id, "delete_job (not implemented)"));
    window.on_open_logs(|id| info!(?id, "open_logs (not implemented)"));
    window.on_theme_changed(|t| info!(theme = %t, "theme_changed"));
    window.on_navigate(|p| info!(page = p, "navigate"));
}

fn refresh_jobs(window: &AppWindow, ctx: Arc<AppContext>) {
    let weak = window.as_weak();
    tokio::spawn(async move {
        let jobs = match ctx.jobs.list().await {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "failed to load jobs");
                return;
            }
        };
        let rows: Vec<crate::JobRow> = jobs.iter().map(job_to_row).collect();
        let count = rows.len() as i32;
        let _ = slint::invoke_from_event_loop(move || {
            let Some(w) = weak.upgrade() else { return };
            let model = Rc::new(VecModel::from(rows));
            w.set_jobs(ModelRc::from(model));
            w.set_total_jobs(count);
        });
    });
}

use std::rc::Rc;
use std::sync::Arc;

use application::commands;
use application::{AppContext, Scheduler};
use domain::{ArchiveConfig, ExcludeRules, Job, JobTrigger, RetentionPolicy};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use time::OffsetDateTime;
use tracing::{info, warn};

use crate::bindings::jobs::{job_to_row, latest_runs_by_job, source_name_map, JobView};
use crate::AppWindow;

pub fn wire(window: &AppWindow, ctx: Arc<AppContext>, sched: Arc<Scheduler>) {
    window.set_jobs(ModelRc::new(VecModel::<crate::JobRow>::default()));
    refresh(window, ctx.clone());

    // ── открыть «+ Новое задание» ─────────────────────────────────
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_add_job(move || {
            let Some(w) = weak.upgrade() else { return };
            // Подгружаем актуальный список источников для ComboBox.
            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let sources = ctx.sources.list().await.unwrap_or_default();
                let names: Vec<SharedString> =
                    sources.iter().map(|s| SharedString::from(s.name.as_str())).collect();
                let first = names.first().cloned().unwrap_or_default();
                let weak2 = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak2.upgrade() {
                        w.set_job_dialog_sources(ModelRc::from(Rc::new(VecModel::from(names))));
                        w.set_job_dialog_name(SharedString::new());
                        w.set_job_dialog_source(first);
                        w.set_job_dialog_enabled(true);
                        w.set_job_dialog_open(true);
                    }
                });
            });
            // suppress unused
            let _ = w;
        });
    }

    // ── save / cancel диалога ─────────────────────────────────────
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_save_job(move || {
            let Some(w) = weak.upgrade() else { return };
            let name = w.get_job_dialog_name().to_string();
            let source_name = w.get_job_dialog_source().to_string();
            let enabled = w.get_job_dialog_enabled();
            if name.trim().is_empty() || source_name.trim().is_empty() {
                return;
            }
            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                // Резолвим имя источника → uuid (имена не гарантированно уникальны,
                // на Stage 1.3 заменим на (id, name)-пары).
                let sources = ctx.sources.list().await.unwrap_or_default();
                let Some(src) = sources.iter().find(|s| s.name == source_name) else {
                    warn!(source_name, "save_job: source not found");
                    return;
                };
                let now = OffsetDateTime::now_utc();
                let job = Job {
                    id: uuid::Uuid::now_v7(),
                    source_id: src.id,
                    name,
                    enabled,
                    archive: ArchiveConfig::default(),
                    retention: RetentionPolicy::default(),
                    exclude: ExcludeRules::default(),
                    pre_cmd: None,
                    post_cmd: None,
                    priority: 0,
                    targets: Vec::new(),
                    created_at: now,
                    updated_at: now,
                };
                match commands::jobs::upsert(&ctx, &job).await {
                    Ok(()) => {
                        info!(job_id = %job.id, "job saved");
                        let weak = weak.clone();
                        let ctx2 = ctx.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = weak.upgrade() {
                                w.set_job_dialog_open(false);
                                refresh_from_event_loop(&w, ctx2);
                            }
                        });
                    }
                    Err(e) => warn!(error = %e, "save job failed"),
                }
            });
        });
    }

    {
        let weak = window.as_weak();
        window.on_cancel_job_dialog(move || {
            if let Some(w) = weak.upgrade() {
                w.set_job_dialog_open(false);
            }
        });
    }

    // ── Run now ───────────────────────────────────────────────────
    {
        let sched = sched.clone();
        let weak = window.as_weak();
        window.on_run_job_now(move |id: SharedString| {
            let Ok(uuid) = uuid::Uuid::parse_str(id.as_str()) else {
                warn!(id = %id, "run_job_now: invalid uuid");
                return;
            };
            info!(job_id = %uuid, "run_job_now requested");
            mark_running_in_model(&weak, &uuid);
            let sched = sched.clone();
            tokio::spawn(async move {
                sched.enqueue(JobTrigger::manual(uuid)).await;
            });
        });
    }

    // ── Delete ────────────────────────────────────────────────────
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_delete_job(move |id: SharedString| {
            let Ok(uuid) = uuid::Uuid::parse_str(id.as_str()) else {
                return;
            };
            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                if let Err(e) = commands::jobs::delete(&ctx, uuid).await {
                    warn!(error = %e, "delete job failed");
                    return;
                }
                let weak = weak.clone();
                let ctx2 = ctx.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        refresh_from_event_loop(&w, ctx2);
                    }
                });
            });
        });
    }

    window.on_edit_job(|id| info!(?id, "edit_job: TODO (Stage 1.3)"));
    window.on_toggle_job(|id| info!(?id, "toggle_job: TODO (Stage 1.3)"));
    window.on_open_logs(|id| info!(?id, "open_logs: TODO (Stage 2)"));

    // ── Подписка на DomainEvent → перерисовка модели ──────────────
    spawn_event_listener(window, ctx);
}

fn refresh(window: &AppWindow, ctx: Arc<AppContext>) {
    let weak = window.as_weak();
    tokio::spawn(async move {
        let (rows, counts) = load_rows_and_counts(&ctx).await;
        let _ = slint::invoke_from_event_loop(move || {
            apply_to_window(weak, rows, counts);
        });
    });
}

fn refresh_from_event_loop(window: &AppWindow, ctx: Arc<AppContext>) {
    let weak = window.as_weak();
    tokio::spawn(async move {
        let (rows, counts) = load_rows_and_counts(&ctx).await;
        let _ = slint::invoke_from_event_loop(move || {
            apply_to_window(weak, rows, counts);
        });
    });
}

fn apply_to_window(weak: slint::Weak<AppWindow>, rows: Vec<crate::JobRow>, counts: DashboardCounts) {
    let Some(w) = weak.upgrade() else { return };
    w.set_jobs(ModelRc::from(Rc::new(VecModel::from(rows))));
    w.set_total_jobs(counts.total);
    w.set_ok_today(counts.ok_today);
    w.set_failed_today(counts.failed_today);
}

struct DashboardCounts {
    total: i32,
    ok_today: i32,
    failed_today: i32,
}

async fn load_rows_and_counts(ctx: &AppContext) -> (Vec<crate::JobRow>, DashboardCounts) {
    let jobs = ctx.jobs.list().await.unwrap_or_default();
    let sources = ctx.sources.list().await.unwrap_or_default();
    let name_map = source_name_map(&sources);

    // Подтянем по 24 последних запуска на каждое задание (для today-счётчиков).
    let mut all_runs = Vec::new();
    for j in &jobs {
        if let Ok(runs) = ctx.jobs.list_runs(j.id, 50).await {
            all_runs.extend(runs);
        }
    }
    let latest = latest_runs_by_job(&all_runs);

    let now = time::OffsetDateTime::now_utc();
    let today_start = now - time::Duration::hours(24);
    let mut ok_today = 0i32;
    let mut failed_today = 0i32;
    for r in &all_runs {
        if r.started_at < today_start {
            continue;
        }
        match r.status {
            domain::JobRunStatus::Success => ok_today += 1,
            domain::JobRunStatus::Failed | domain::JobRunStatus::Interrupted => failed_today += 1,
            _ => {}
        }
    }

    let rows = jobs
        .iter()
        .map(|job| {
            let view = JobView {
                job,
                source_name: name_map
                    .get(&job.source_id)
                    .map(String::as_str)
                    .unwrap_or("(удалён)"),
                latest_run: latest.get(&job.id).copied(),
            };
            job_to_row(&view)
        })
        .collect();

    (
        rows,
        DashboardCounts {
            total: jobs.len() as i32,
            ok_today,
            failed_today,
        },
    )
}

/// Помечаем карточку как «выполняется» сразу при клике, не дожидаясь события из БД.
fn mark_running_in_model(weak: &slint::Weak<AppWindow>, job_id: &uuid::Uuid) {
    let Some(w) = weak.upgrade() else { return };
    let model = w.get_jobs();
    for i in 0..model.row_count() {
        let Some(row) = model.row_data(i) else { continue };
        if row.id.as_str() == job_id.to_string() {
            let mut updated = row;
            updated.status = SharedString::from("running");
            updated.status_label = SharedString::from("выполняется");
            updated.progress = 0.0;
            model.set_row_data(i, updated);
            break;
        }
    }
}

fn spawn_event_listener(window: &AppWindow, ctx: Arc<AppContext>) {
    let weak = window.as_weak();
    let mut rx = ctx.events.subscribe();
    let ctx2 = ctx.clone();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            use domain::DomainEvent::*;
            let weak = weak.clone();
            let ctx_inner = ctx2.clone();
            match event {
                JobRunStarted(_) | JobRunFinished(_) | JobChanged { .. } => {
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak.upgrade() {
                            refresh_from_event_loop(&w, ctx_inner);
                        }
                    });
                }
                StageProgress(p) => {
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak.upgrade() {
                            update_progress_in_model(&w, p.job_id, p.percent);
                        }
                    });
                }
                _ => {}
            }
        }
    });
}

fn update_progress_in_model(w: &AppWindow, job_id: uuid::Uuid, percent: f32) {
    let model = w.get_jobs();
    for i in 0..model.row_count() {
        let Some(row) = model.row_data(i) else { continue };
        if row.id.as_str() == job_id.to_string() {
            let mut updated = row;
            updated.progress = percent.clamp(0.0, 1.0);
            model.set_row_data(i, updated);
            break;
        }
    }
}

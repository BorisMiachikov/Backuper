use std::rc::Rc;
use std::sync::Arc;

use application::commands;
use application::{AppContext, Scheduler};
use domain::{ArchiveConfig, ExcludeRules, Job, JobTrigger, RetentionPolicy, Schedule, ScheduleKind};
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
                        w.set_job_dialog_id(SharedString::new());
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
            let editing_id = w.get_job_dialog_id().to_string();
            if name.trim().is_empty() || source_name.trim().is_empty() {
                return;
            }
            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let sources = ctx.sources.list().await.unwrap_or_default();
                let Some(src) = sources.iter().find(|s| s.name == source_name) else {
                    warn!(source_name, "save_job: source not found");
                    return;
                };
                let now = OffsetDateTime::now_utc();
                let job = if editing_id.is_empty() {
                    Job {
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
                    }
                } else {
                    let Ok(edit_uuid) = uuid::Uuid::parse_str(&editing_id) else {
                        warn!(editing_id, "save_job: invalid uuid");
                        return;
                    };
                    match ctx.jobs.get(edit_uuid).await {
                        Ok(Some(existing)) => Job {
                            id: existing.id,
                            source_id: src.id,
                            name,
                            enabled,
                            updated_at: now,
                            ..existing
                        },
                        _ => {
                            warn!(%edit_uuid, "save_job edit: job not found");
                            return;
                        }
                    }
                };
                match commands::jobs::upsert(&ctx, &job).await {
                    Ok(()) => {
                        info!(job_id = %job.id, "job saved");
                        let weak = weak.clone();
                        let ctx2 = ctx.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = weak.upgrade() {
                                w.set_job_dialog_id(SharedString::new());
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
                w.set_job_dialog_id(SharedString::new());
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

    // ── Edit job ──────────────────────────────────────────────────
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_edit_job(move |id: SharedString| {
            let Ok(uuid) = uuid::Uuid::parse_str(id.as_str()) else {
                warn!(%id, "edit_job: invalid uuid");
                return;
            };
            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let Ok(Some(job)) = ctx.jobs.get(uuid).await else {
                    warn!(%uuid, "edit_job: not found");
                    return;
                };
                let sources = ctx.sources.list().await.unwrap_or_default();
                let source_name = sources
                    .iter()
                    .find(|s| s.id == job.source_id)
                    .map(|s| SharedString::from(s.name.as_str()))
                    .unwrap_or_default();
                let names: Vec<SharedString> =
                    sources.iter().map(|s| SharedString::from(s.name.as_str())).collect();
                let job_id_str = SharedString::from(job.id.to_string());
                let job_name = SharedString::from(job.name.as_str());
                let job_enabled = job.enabled;
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        w.set_job_dialog_sources(ModelRc::from(Rc::new(VecModel::from(names))));
                        w.set_job_dialog_id(job_id_str);
                        w.set_job_dialog_name(job_name);
                        w.set_job_dialog_source(source_name);
                        w.set_job_dialog_enabled(job_enabled);
                        w.set_job_dialog_open(true);
                    }
                });
            });
        });
    }

    // ── Toggle job enabled ────────────────────────────────────────
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_toggle_job(move |id: SharedString| {
            let Ok(uuid) = uuid::Uuid::parse_str(id.as_str()) else {
                warn!(%id, "toggle_job: invalid uuid");
                return;
            };
            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let Ok(Some(mut job)) = ctx.jobs.get(uuid).await else {
                    warn!(%uuid, "toggle_job: not found");
                    return;
                };
                job.enabled = !job.enabled;
                job.updated_at = OffsetDateTime::now_utc();
                if let Err(e) = commands::jobs::upsert(&ctx, &job).await {
                    warn!(error = %e, "toggle_job failed");
                    return;
                }
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        refresh_from_event_loop(&w, ctx);
                    }
                });
            });
        });
    }

    window.on_open_logs(|id| info!(?id, "open_logs: TODO (Stage 2)"));

    // ── Открыть диалог расписания ─────────────────────────────────
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_open_schedule(move |id: SharedString| {
            let Ok(uuid) = uuid::Uuid::parse_str(id.as_str()) else {
                warn!(%id, "open_schedule: invalid uuid");
                return;
            };
            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let Ok(Some(job)) = ctx.jobs.get(uuid).await else {
                    warn!(%uuid, "open_schedule: job not found");
                    return;
                };
                // Ищем существующее расписание для этого job
                let schedules = ctx.jobs.list_schedules().await.unwrap_or_default();
                let existing = schedules.into_iter().find(|s| s.job_id == uuid);

                let job_name = SharedString::from(job.name.as_str());
                let job_id_str = SharedString::from(uuid.to_string());

                // Определяем текущие значения из существующего расписания
                let (kind, every_min, d_h, d_m, w_wd, w_h, w_m, cron) =
                    if let Some(ref s) = existing {
                        match &s.kind {
                            ScheduleKind::EveryMinutes { minutes } =>
                                ("every_minutes", *minutes as i32, 2, 0, 1, 2, 0, "0 2 * * *".to_string()),
                            ScheduleKind::Daily { hour, minute } =>
                                ("daily", 60, *hour as i32, *minute as i32, 1, 2, 0, "0 2 * * *".to_string()),
                            ScheduleKind::Weekly { weekday, hour, minute } =>
                                ("weekly", 60, 2, 0, *weekday as i32, *hour as i32, *minute as i32, "0 2 * * *".to_string()),
                            ScheduleKind::Cron { expression } =>
                                ("cron", 60, 2, 0, 1, 2, 0, expression.clone()),
                        }
                    } else {
                        ("none", 60, 2, 0, 1, 2, 0, "0 2 * * *".to_string())
                    };

                let kind = SharedString::from(kind);
                let cron = SharedString::from(cron);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        w.set_sched_dialog_job_id(job_id_str);
                        w.set_sched_dialog_job_name(job_name);
                        w.set_sched_dialog_kind(kind);
                        w.set_sched_dialog_every_minutes(every_min);
                        w.set_sched_dialog_daily_hour(d_h);
                        w.set_sched_dialog_daily_minute(d_m);
                        w.set_sched_dialog_weekly_weekday(w_wd);
                        w.set_sched_dialog_weekly_hour(w_h);
                        w.set_sched_dialog_weekly_minute(w_m);
                        w.set_sched_dialog_cron_expr(cron);
                        w.set_sched_dialog_open(true);
                    }
                });
            });
        });
    }

    // ── Сохранить расписание ──────────────────────────────────────
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_save_schedule(move || {
            let Some(w) = weak.upgrade() else { return };
            let job_id_str = w.get_sched_dialog_job_id().to_string();
            let kind_str   = w.get_sched_dialog_kind().to_string();
            let every_min  = w.get_sched_dialog_every_minutes() as u32;
            let d_h        = w.get_sched_dialog_daily_hour() as u8;
            let d_m        = w.get_sched_dialog_daily_minute() as u8;
            let w_wd       = w.get_sched_dialog_weekly_weekday() as u8;
            let w_h        = w.get_sched_dialog_weekly_hour() as u8;
            let w_m        = w.get_sched_dialog_weekly_minute() as u8;
            let cron_expr  = w.get_sched_dialog_cron_expr().to_string();

            let Ok(job_uuid) = uuid::Uuid::parse_str(&job_id_str) else { return };

            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                // Найти или создать Schedule для этого job
                let schedules = ctx.jobs.list_schedules().await.unwrap_or_default();
                let existing_id = schedules.iter()
                    .find(|s| s.job_id == job_uuid)
                    .map(|s| s.id);

                if kind_str == "none" {
                    // Удаляем расписание — не трогаем (просто ставим enabled=false через upsert)
                    if let Some(id) = existing_id {
                        let mut s = schedules.into_iter().find(|s| s.id == id).unwrap();
                        s.enabled = false;
                        s.next_fire = None;
                        let _ = ctx.jobs.upsert_schedule(&s).await;
                    }
                } else {
                    let kind = match kind_str.as_str() {
                        "every_minutes" => ScheduleKind::EveryMinutes { minutes: every_min },
                        "daily"         => ScheduleKind::Daily { hour: d_h, minute: d_m },
                        "weekly"        => ScheduleKind::Weekly { weekday: w_wd, hour: w_h, minute: w_m },
                        _               => ScheduleKind::Cron { expression: cron_expr },
                    };
                    let mut sched = if let Some(id) = existing_id {
                        schedules.into_iter().find(|s| s.id == id).unwrap()
                    } else {
                        Schedule::new(job_uuid, kind.clone())
                    };
                    sched.kind = kind;
                    sched.enabled = true;
                    sched.next_fire = None; // сбрасываем — poll_schedules пересчитает
                    if let Err(e) = ctx.jobs.upsert_schedule(&sched).await {
                        warn!(error = %e, "save_schedule failed");
                        return;
                    }
                    info!(job_id = %job_uuid, "schedule saved");
                }

                // После сохранения — подождём один тик планировщика (next_fire пересчитается),
                // потом обновим Dashboard.
                tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                let (rows, counts, recent) = load_rows_and_counts(&ctx).await;
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        w.set_sched_dialog_open(false);
                        apply_to_window(w.as_weak(), rows, counts, recent);
                    }
                });
            });
        });
    }

    // ── Отмена диалога расписания ─────────────────────────────────
    {
        let weak = window.as_weak();
        window.on_cancel_schedule_dialog(move || {
            if let Some(w) = weak.upgrade() {
                w.set_sched_dialog_open(false);
            }
        });
    }

    // ── Подписка на DomainEvent → перерисовка модели ──────────────
    spawn_event_listener(window, ctx);
}

fn refresh(window: &AppWindow, ctx: Arc<AppContext>) {
    let weak = window.as_weak();
    tokio::spawn(async move {
        let (rows, counts, recent) = load_rows_and_counts(&ctx).await;
        let _ = slint::invoke_from_event_loop(move || {
            apply_to_window(weak, rows, counts, recent);
        });
    });
}

fn refresh_from_event_loop(window: &AppWindow, ctx: Arc<AppContext>) {
    let weak = window.as_weak();
    tokio::spawn(async move {
        let (rows, counts, recent) = load_rows_and_counts(&ctx).await;
        let _ = slint::invoke_from_event_loop(move || {
            apply_to_window(weak, rows, counts, recent);
        });
    });
}

fn apply_to_window(
    weak: slint::Weak<AppWindow>,
    rows: Vec<crate::JobRow>,
    counts: DashboardCounts,
    recent: Vec<crate::RunRow>,
) {
    let Some(w) = weak.upgrade() else { return };
    w.set_jobs(ModelRc::from(Rc::new(VecModel::from(rows))));
    w.set_total_jobs(counts.total);
    w.set_ok_today(counts.ok_today);
    w.set_failed_today(counts.failed_today);
    w.set_next_run(SharedString::from(counts.next_run));
    w.set_recent_runs(ModelRc::from(Rc::new(VecModel::from(recent))));
}

struct DashboardCounts {
    total: i32,
    ok_today: i32,
    failed_today: i32,
    next_run: String,
}

async fn load_rows_and_counts(
    ctx: &AppContext,
) -> (Vec<crate::JobRow>, DashboardCounts, Vec<crate::RunRow>) {
    let jobs = ctx.jobs.list().await.unwrap_or_default();
    let sources = ctx.sources.list().await.unwrap_or_default();
    let name_map = source_name_map(&sources);

    let mut all_runs = Vec::new();
    for j in &jobs {
        if let Ok(runs) = ctx.jobs.list_runs(j.id, 50).await {
            all_runs.extend(runs);
        }
    }
    let latest = latest_runs_by_job(&all_runs);

    let now = OffsetDateTime::now_utc();
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

    // Последние 10 запусков для Dashboard (сортировка по убыванию started_at).
    let mut sorted_runs = all_runs.clone();
    sorted_runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    let job_name_map: std::collections::HashMap<uuid::Uuid, &str> = jobs
        .iter()
        .map(|j| (j.id, j.name.as_str()))
        .collect();
    let recent: Vec<crate::RunRow> = sorted_runs
        .iter()
        .take(10)
        .map(|r| {
            let job_name = job_name_map.get(&r.job_id).copied().unwrap_or("(удалено)");
            let (status, label) = match r.status {
                domain::JobRunStatus::Success => ("ok", "успешно"),
                domain::JobRunStatus::Failed => ("error", "ошибка"),
                domain::JobRunStatus::Interrupted => ("error", "прервано"),
                domain::JobRunStatus::Running => ("running", "выполняется"),
                _ => ("idle", "—"),
            };
            let duration = if let Some(fin) = r.finished_at {
                let secs = (fin - r.started_at).whole_seconds().max(0) as u64;
                if secs < 60 {
                    format!("{secs} сек")
                } else {
                    format!("{} мин {} сек", secs / 60, secs % 60)
                }
            } else {
                "—".to_string()
            };
            let when = render_relative_time(r.started_at, now);
            crate::RunRow {
                job_name: SharedString::from(job_name),
                status: SharedString::from(status),
                status_label: SharedString::from(label),
                when: SharedString::from(when),
                duration: SharedString::from(duration),
            }
        })
        .collect();

    // Расписания: карта job_id → строка следующего запуска + минимум для Dashboard.
    let schedules = ctx.jobs.list_schedules().await.unwrap_or_default();
    let next_fire_map: std::collections::HashMap<uuid::Uuid, String> = schedules
        .iter()
        .filter(|s| s.enabled)
        .filter_map(|s| s.next_fire.filter(|&t| t >= now).map(|t| (s.job_id, render_next_fire(t, now))))
        .collect();

    let rows = jobs
        .iter()
        .map(|job| {
            let next_run_at = next_fire_map.get(&job.id).map(String::as_str).unwrap_or("");
            let view = JobView {
                job,
                source_name: name_map
                    .get(&job.source_id)
                    .map(String::as_str)
                    .unwrap_or("(удалён)"),
                latest_run: latest.get(&job.id).copied(),
                next_run_at,
            };
            job_to_row(&view)
        })
        .collect();

    let next_run = schedules
        .iter()
        .filter_map(|s| s.next_fire)
        .filter(|&t| t >= now)
        .min()
        .map(|t| render_next_fire(t, now))
        .unwrap_or_else(|| "—".to_string());

    (
        rows,
        DashboardCounts { total: jobs.len() as i32, ok_today, failed_today, next_run },
        recent,
    )
}

fn render_next_fire(t: OffsetDateTime, now: OffsetDateTime) -> String {
    let secs = (t - now).whole_seconds().max(0) as u64;
    match secs {
        0..=59   => format!("через {secs} сек"),
        60..=3599 => format!("через {} мин", secs / 60),
        3600..=86399 => {
            let local = t.to_offset(time::UtcOffset::from_hms(3, 0, 0).unwrap_or(time::UtcOffset::UTC));
            format!("сегодня в {:02}:{:02}", local.hour(), local.minute())
        }
        _ => {
            let local = t.to_offset(time::UtcOffset::from_hms(3, 0, 0).unwrap_or(time::UtcOffset::UTC));
            format!("{:02}.{:02} в {:02}:{:02}", local.day(), local.month() as u8, local.hour(), local.minute())
        }
    }
}

fn render_relative_time(ts: OffsetDateTime, now: OffsetDateTime) -> String {
    let secs = (now - ts).whole_seconds().max(0) as u64;
    match secs {
        0..=59 => format!("{secs} сек назад"),
        60..=3599 => format!("{} мин назад", secs / 60),
        3600..=86399 => format!("{} ч назад", secs / 3600),
        _ => format!("{} д назад", secs / 86400),
    }
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

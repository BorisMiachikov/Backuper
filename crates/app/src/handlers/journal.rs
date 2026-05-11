use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use application::AppContext;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use time::OffsetDateTime;
use tracing::warn;
use uuid::Uuid;

use crate::AppWindow;

pub fn wire(window: &AppWindow, ctx: Arc<AppContext>) {
    window.set_journal_runs(ModelRc::new(VecModel::<crate::JournalRow>::default()));
    refresh(window, ctx.clone());
    spawn_event_listener(window, ctx);
}

fn spawn_event_listener(window: &AppWindow, ctx: Arc<AppContext>) {
    let weak = window.as_weak();
    let mut rx = ctx.events.subscribe();
    let ctx2 = ctx.clone();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if matches!(event, domain::DomainEvent::JobRunFinished(_)) {
                let weak = weak.clone();
                let ctx_inner = ctx2.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        refresh(&w, ctx_inner);
                    }
                });
            }
        }
    });
}

pub fn refresh(window: &AppWindow, ctx: Arc<AppContext>) {
    let weak = window.as_weak();
    tokio::spawn(async move {
        load_and_apply(weak, ctx).await;
    });
}

async fn load_and_apply(weak: Weak<AppWindow>, ctx: Arc<AppContext>) {
    let runs = match ctx.jobs.list_all_runs(200).await {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "failed to load all runs");
            return;
        }
    };
    let jobs = ctx.jobs.list().await.unwrap_or_default();
    let name_map: HashMap<Uuid, &str> = jobs.iter().map(|j| (j.id, j.name.as_str())).collect();
    let now = OffsetDateTime::now_utc();

    let rows: Vec<crate::JournalRow> = runs
        .iter()
        .map(|r| {
            let job_name = name_map.get(&r.job_id).copied().unwrap_or("(удалено)");
            let (status, label) = match r.status {
                domain::JobRunStatus::Success     => ("ok", "успешно"),
                domain::JobRunStatus::Failed      => ("error", "ошибка"),
                domain::JobRunStatus::Interrupted => ("error", "прервано"),
                domain::JobRunStatus::Running     => ("running", "выполняется"),
                domain::JobRunStatus::Cancelled   => ("idle", "отменено"),
                _                                 => ("idle", "—"),
            };
            let duration = if let Some(fin) = r.finished_at {
                let secs = (fin - r.started_at).whole_seconds().max(0) as u64;
                if secs < 60 { format!("{secs} сек") } else { format!("{} мин {} сек", secs / 60, secs % 60) }
            } else {
                "—".to_owned()
            };
            let bytes_out = if r.bytes_out > 0 {
                format_bytes(r.bytes_out)
            } else {
                String::new()
            };
            let started_at = render_relative_time(r.started_at, now);
            crate::JournalRow {
                id: SharedString::from(r.id.to_string()),
                job_name: SharedString::from(job_name),
                status: SharedString::from(status),
                status_label: SharedString::from(label),
                started_at: SharedString::from(started_at),
                duration: SharedString::from(duration),
                bytes_out: SharedString::from(bytes_out),
                error_msg: SharedString::from(r.error_msg.as_deref().unwrap_or("")),
            }
        })
        .collect();

    let _ = slint::invoke_from_event_loop(move || {
        if let Some(w) = weak.upgrade() {
            w.set_journal_runs(ModelRc::from(Rc::new(VecModel::from(rows))));
        }
    });
}

fn render_relative_time(ts: OffsetDateTime, now: OffsetDateTime) -> String {
    let secs = (now - ts).whole_seconds().max(0) as u64;
    match secs {
        0..=59     => format!("{secs} сек назад"),
        60..=3599  => format!("{} мин назад", secs / 60),
        3600..=86399 => format!("{} ч назад", secs / 3600),
        _          => format!("{} д назад", secs / 86400),
    }
}

fn format_bytes(b: u64) -> String {
    if b < 1024 {
        format!("{b} Б")
    } else if b < 1024 * 1024 {
        format!("{:.1} КБ", b as f64 / 1024.0)
    } else if b < 1024 * 1024 * 1024 {
        format!("{:.1} МБ", b as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} ГБ", b as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

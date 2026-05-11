use std::collections::HashMap;

use domain::{Job, JobRun, JobRunStatus, Source};
use slint::SharedString;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

pub struct JobView<'a> {
    pub job: &'a Job,
    pub source_name: &'a str,
    pub latest_run: Option<&'a JobRun>,
    pub next_run_at: &'a str,
}

pub fn job_to_row(view: &JobView<'_>) -> crate::JobRow {
    let (status, label) = render_status(view.job.enabled, view.latest_run);
    crate::JobRow {
        id: SharedString::from(view.job.id.to_string()),
        name: SharedString::from(view.job.name.as_str()),
        source_name: SharedString::from(view.source_name),
        last_run: SharedString::from(render_last_run(view.latest_run)),
        next_run_at: SharedString::from(view.next_run_at),
        status: SharedString::from(status),
        status_label: SharedString::from(label),
        progress: 0.0,
        enabled: view.job.enabled,
    }
}

fn render_status(enabled: bool, latest: Option<&JobRun>) -> (&'static str, &'static str) {
    if !enabled {
        return ("disabled", "выключено");
    }
    match latest.map(|r| r.status) {
        Some(JobRunStatus::Running | JobRunStatus::Pending) => ("running", "выполняется"),
        Some(JobRunStatus::Success) => ("ok", "успешно"),
        Some(JobRunStatus::Failed | JobRunStatus::Interrupted) => ("error", "ошибка"),
        Some(JobRunStatus::Cancelled) => ("idle", "отменено"),
        Some(JobRunStatus::Skipped) => ("idle", "пропущено"),
        None => ("idle", "ни разу не запускалось"),
    }
}

fn render_last_run(run: Option<&JobRun>) -> String {
    let Some(r) = run else { return "—".into() };
    let ts = r.finished_at.unwrap_or(r.started_at);
    format_relative(ts)
}

/// Простое относительное представление времени: "только что", "5 мин назад",
/// "2 ч назад", иначе RFC3339-дата. Stage 6 — настоящая i18n.
pub fn format_relative(ts: OffsetDateTime) -> String {
    let now = OffsetDateTime::now_utc();
    let delta = now - ts;
    let secs = delta.whole_seconds();
    if (0..60).contains(&secs) {
        return "только что".into();
    }
    if (60..3600).contains(&secs) {
        return format!("{} мин назад", secs / 60);
    }
    if (3600..86_400).contains(&secs) {
        return format!("{} ч назад", secs / 3600);
    }
    if (86_400..604_800).contains(&secs) {
        return format!("{} дн назад", secs / 86_400);
    }
    ts.format(&Rfc3339).unwrap_or_else(|_| "—".into())
}

/// Индекс «source_id → последний JobRun» по плоскому списку запусков.
pub fn latest_runs_by_job(runs: &[JobRun]) -> HashMap<Uuid, &JobRun> {
    let mut out: HashMap<Uuid, &JobRun> = HashMap::new();
    for r in runs {
        out.entry(r.job_id)
            .and_modify(|prev| {
                if r.started_at > prev.started_at {
                    *prev = r;
                }
            })
            .or_insert(r);
    }
    out
}

pub fn source_name_map(sources: &[Source]) -> HashMap<Uuid, String> {
    sources.iter().map(|s| (s.id, s.name.clone())).collect()
}

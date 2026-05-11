//! ScheduleKind ↔ (discriminator, cron_expr, run_at) для таблицы `schedules`.
//!
//! Конвенция кодирования:
//! - `every_minutes`: cron_expr = N (число минут), run_at = NULL
//! - `daily`:         cron_expr = NULL, run_at = "HH:MM"
//! - `weekly`:        cron_expr = NULL, run_at = "W HH:MM" (W = 0-6, 0=Sunday)
//! - `cron`:          cron_expr = выражение, run_at = NULL

use domain::ScheduleKind;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScheduleError {
    #[error("unknown schedule discriminator: {0}")]
    UnknownKind(String),
    #[error("missing required field {0} for kind {1}")]
    MissingField(&'static str, &'static str),
    #[error("invalid value for {field}: {message}")]
    InvalidValue {
        field: &'static str,
        message: String,
    },
}

pub const KIND_EVERY_MINUTES: &str = "every_minutes";
pub const KIND_DAILY: &str = "daily";
pub const KIND_WEEKLY: &str = "weekly";
pub const KIND_CRON: &str = "cron";

pub struct ScheduleColumns {
    pub kind: String,
    pub cron_expr: Option<String>,
    pub run_at: Option<String>,
}

pub fn split(kind: &ScheduleKind) -> ScheduleColumns {
    match kind {
        ScheduleKind::EveryMinutes { minutes } => ScheduleColumns {
            kind: KIND_EVERY_MINUTES.into(),
            cron_expr: Some(minutes.to_string()),
            run_at: None,
        },
        ScheduleKind::Daily { hour, minute } => ScheduleColumns {
            kind: KIND_DAILY.into(),
            cron_expr: None,
            run_at: Some(format!("{hour:02}:{minute:02}")),
        },
        ScheduleKind::Weekly {
            weekday,
            hour,
            minute,
        } => ScheduleColumns {
            kind: KIND_WEEKLY.into(),
            cron_expr: None,
            run_at: Some(format!("{weekday} {hour:02}:{minute:02}")),
        },
        ScheduleKind::Cron { expression } => ScheduleColumns {
            kind: KIND_CRON.into(),
            cron_expr: Some(expression.clone()),
            run_at: None,
        },
    }
}

pub fn assemble(
    kind: &str,
    cron_expr: Option<&str>,
    run_at: Option<&str>,
) -> Result<ScheduleKind, ScheduleError> {
    match kind {
        KIND_EVERY_MINUTES => {
            let raw = cron_expr.ok_or(ScheduleError::MissingField("cron_expr", KIND_EVERY_MINUTES))?;
            let minutes = raw.trim().parse::<u32>().map_err(|e| ScheduleError::InvalidValue {
                field: "cron_expr",
                message: e.to_string(),
            })?;
            Ok(ScheduleKind::EveryMinutes { minutes })
        }
        KIND_DAILY => {
            let raw = run_at.ok_or(ScheduleError::MissingField("run_at", KIND_DAILY))?;
            let (h, m) = parse_hh_mm(raw)?;
            Ok(ScheduleKind::Daily { hour: h, minute: m })
        }
        KIND_WEEKLY => {
            let raw = run_at.ok_or(ScheduleError::MissingField("run_at", KIND_WEEKLY))?;
            let (weekday, h, m) = parse_w_hh_mm(raw)?;
            Ok(ScheduleKind::Weekly {
                weekday,
                hour: h,
                minute: m,
            })
        }
        KIND_CRON => {
            let raw = cron_expr.ok_or(ScheduleError::MissingField("cron_expr", KIND_CRON))?;
            Ok(ScheduleKind::Cron {
                expression: raw.to_owned(),
            })
        }
        other => Err(ScheduleError::UnknownKind(other.into())),
    }
}

fn parse_hh_mm(raw: &str) -> Result<(u8, u8), ScheduleError> {
    let (h, m) = raw
        .split_once(':')
        .ok_or_else(|| ScheduleError::InvalidValue {
            field: "run_at",
            message: format!("expected HH:MM, got {raw}"),
        })?;
    let hour: u8 = h.trim().parse().map_err(|_| ScheduleError::InvalidValue {
        field: "run_at",
        message: format!("hour: {h}"),
    })?;
    let minute: u8 = m.trim().parse().map_err(|_| ScheduleError::InvalidValue {
        field: "run_at",
        message: format!("minute: {m}"),
    })?;
    if hour > 23 || minute > 59 {
        return Err(ScheduleError::InvalidValue {
            field: "run_at",
            message: format!("out of range: {hour:02}:{minute:02}"),
        });
    }
    Ok((hour, minute))
}

fn parse_w_hh_mm(raw: &str) -> Result<(u8, u8, u8), ScheduleError> {
    let (w, rest) = raw
        .split_once(' ')
        .ok_or_else(|| ScheduleError::InvalidValue {
            field: "run_at",
            message: format!("expected 'W HH:MM', got {raw}"),
        })?;
    let weekday: u8 = w.trim().parse().map_err(|_| ScheduleError::InvalidValue {
        field: "run_at",
        message: format!("weekday: {w}"),
    })?;
    if weekday > 6 {
        return Err(ScheduleError::InvalidValue {
            field: "run_at",
            message: format!("weekday out of range: {weekday}"),
        });
    }
    let (h, m) = parse_hh_mm(rest)?;
    Ok((weekday, h, m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(k: ScheduleKind) {
        let cols = split(&k);
        let back = assemble(&cols.kind, cols.cron_expr.as_deref(), cols.run_at.as_deref()).unwrap();
        assert_eq!(k, back);
    }

    #[test]
    fn roundtrip_every_minutes() {
        roundtrip(ScheduleKind::EveryMinutes { minutes: 15 });
    }

    #[test]
    fn roundtrip_daily() {
        roundtrip(ScheduleKind::Daily {
            hour: 3,
            minute: 30,
        });
    }

    #[test]
    fn roundtrip_weekly() {
        roundtrip(ScheduleKind::Weekly {
            weekday: 6,
            hour: 23,
            minute: 59,
        });
    }

    #[test]
    fn roundtrip_cron() {
        roundtrip(ScheduleKind::Cron {
            expression: "0 2 * * 1-5".into(),
        });
    }

    #[test]
    fn unknown_kind_rejected() {
        assert!(matches!(
            assemble("bogus", None, None),
            Err(ScheduleError::UnknownKind(_))
        ));
    }

    #[test]
    fn invalid_hour_rejected() {
        assert!(matches!(
            assemble(KIND_DAILY, None, Some("99:00")),
            Err(ScheduleError::InvalidValue { .. })
        ));
    }

    #[test]
    fn invalid_weekday_rejected() {
        assert!(matches!(
            assemble(KIND_WEEKLY, None, Some("9 10:00")),
            Err(ScheduleError::InvalidValue { .. })
        ));
    }
}

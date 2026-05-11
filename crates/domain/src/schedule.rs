use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleKind {
    EveryMinutes {
        minutes: u32,
    },
    Daily {
        hour: u8,
        minute: u8,
    },
    Weekly {
        weekday: u8, // 0=Sunday..6=Saturday
        hour: u8,
        minute: u8,
    },
    Cron {
        expression: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub id: Uuid,
    pub job_id: Uuid,
    pub kind: ScheduleKind,
    pub enabled: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    pub next_fire: Option<OffsetDateTime>,
}

impl Schedule {
    pub fn new(job_id: Uuid, kind: ScheduleKind) -> Self {
        Self {
            id: Uuid::now_v7(),
            job_id,
            kind,
            enabled: true,
            next_fire: None,
        }
    }
}

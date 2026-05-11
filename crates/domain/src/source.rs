use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceKind {
    OneCFile,
    OneCServer {
        server: String,
        ref_base: String,
        cluster_port: Option<u16>,
    },
    Folder,
    Files {
        include: Vec<String>,
    },
}

pub type SourceTag = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: Uuid,
    pub kind: SourceKind,
    pub name: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub description: Option<String>,
    pub tags: Vec<SourceTag>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl Source {
    pub fn new(kind: SourceKind, name: impl Into<String>, path: PathBuf) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            id: Uuid::now_v7(),
            kind,
            name: name.into(),
            path,
            enabled: true,
            description: None,
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> crate::DomainResult<()> {
        if self.name.trim().is_empty() {
            return Err(crate::DomainError::Validation("source.name is empty".into()));
        }
        if self.path.as_os_str().is_empty() {
            return Err(crate::DomainError::Validation("source.path is empty".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_empty_name() {
        let mut s = Source::new(SourceKind::Folder, "  ", PathBuf::from("D:/x"));
        assert!(s.validate().is_err());
        s.name = "ok".into();
        assert!(s.validate().is_ok());
    }
}

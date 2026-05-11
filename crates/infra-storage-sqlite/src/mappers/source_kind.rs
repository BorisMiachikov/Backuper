use domain::SourceKind;
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssembleError {
    #[error("unknown kind discriminator: {0}")]
    UnknownKind(String),
    #[error("missing field {field} for kind {kind}")]
    MissingField { kind: &'static str, field: &'static str },
    #[error("invalid field type for {field}: {message}")]
    InvalidType { field: &'static str, message: String },
}

pub const KIND_ONE_C_FILE: &str = "one_c_file";
pub const KIND_ONE_C_SERVER: &str = "one_c_server";
pub const KIND_FOLDER: &str = "folder";
pub const KIND_FILES: &str = "files";

/// Разбивает `SourceKind` в пару (discriminator, params).
pub fn split(kind: &SourceKind) -> (String, Value) {
    match kind {
        SourceKind::OneCFile => (KIND_ONE_C_FILE.into(), json!({})),
        SourceKind::OneCServer {
            server,
            ref_base,
            cluster_port,
        } => (
            KIND_ONE_C_SERVER.into(),
            json!({
                "server": server,
                "ref_base": ref_base,
                "cluster_port": cluster_port,
            }),
        ),
        SourceKind::Folder => (KIND_FOLDER.into(), json!({})),
        SourceKind::Files { include } => (
            KIND_FILES.into(),
            json!({ "include": include }),
        ),
    }
}

/// Собирает `SourceKind` из строки-дискриминатора и JSON-параметров.
pub fn assemble(kind: &str, params: &Value) -> Result<SourceKind, AssembleError> {
    match kind {
        KIND_ONE_C_FILE => Ok(SourceKind::OneCFile),
        KIND_FOLDER => Ok(SourceKind::Folder),
        KIND_ONE_C_SERVER => Ok(SourceKind::OneCServer {
            server: take_string(params, "server", KIND_ONE_C_SERVER)?,
            ref_base: take_string(params, "ref_base", KIND_ONE_C_SERVER)?,
            cluster_port: params
                .get("cluster_port")
                .and_then(|v| {
                    if v.is_null() {
                        None
                    } else {
                        v.as_u64().map(|n| u16::try_from(n).ok())
                    }
                })
                .flatten(),
        }),
        KIND_FILES => Ok(SourceKind::Files {
            include: params
                .get("include")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        }),
        other => Err(AssembleError::UnknownKind(other.to_owned())),
    }
}

fn take_string(
    params: &Value,
    field: &'static str,
    kind: &'static str,
) -> Result<String, AssembleError> {
    params
        .get(field)
        .ok_or(AssembleError::MissingField { kind, field })
        .and_then(|v| {
            v.as_str().map(String::from).ok_or(AssembleError::InvalidType {
                field,
                message: "expected string".into(),
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_unit_variants() {
        for k in [SourceKind::OneCFile, SourceKind::Folder] {
            let (disc, params) = split(&k);
            let back = assemble(&disc, &params).unwrap();
            assert_eq!(k, back);
        }
    }

    #[test]
    fn roundtrip_server() {
        let k = SourceKind::OneCServer {
            server: "1c-srv01.local".into(),
            ref_base: "Бухгалтерия".into(),
            cluster_port: Some(1541),
        };
        let (disc, params) = split(&k);
        assert_eq!(disc, KIND_ONE_C_SERVER);
        let back = assemble(&disc, &params).unwrap();
        assert_eq!(k, back);
    }

    #[test]
    fn roundtrip_files() {
        let k = SourceKind::Files {
            include: vec!["a.txt".into(), "b/*.log".into()],
        };
        let (disc, params) = split(&k);
        let back = assemble(&disc, &params).unwrap();
        assert_eq!(k, back);
    }

    #[test]
    fn unknown_kind_rejected() {
        assert!(matches!(
            assemble("bogus", &json!({})),
            Err(AssembleError::UnknownKind(_))
        ));
    }
}

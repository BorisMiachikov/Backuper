use domain::{Source, SourceKind};
use slint::SharedString;

pub fn source_to_row(s: &Source) -> crate::SourceRow {
    crate::SourceRow {
        id: SharedString::from(s.id.to_string()),
        name: SharedString::from(s.name.as_str()),
        kind: SharedString::from(kind_label(&s.kind)),
        path: SharedString::from(s.path.to_string_lossy().as_ref()),
        description: SharedString::from(s.description.clone().unwrap_or_default()),
        tags: SharedString::from(s.tags.join(", ")),
        enabled: s.enabled,
    }
}

pub fn kind_label(kind: &SourceKind) -> &'static str {
    match kind {
        SourceKind::OneCFile => "1С (файловая)",
        SourceKind::OneCServer { .. } => "1С (серверная)",
        SourceKind::Folder => "папка",
        SourceKind::Files { .. } => "файлы",
    }
}

pub fn kind_from_combo(value: &str) -> SourceKind {
    match value {
        "one_c_file" => SourceKind::OneCFile,
        "one_c_server" => SourceKind::OneCServer {
            server: String::new(),
            ref_base: String::new(),
            cluster_port: None,
        },
        "files" => SourceKind::Files { include: Vec::new() },
        _ => SourceKind::Folder,
    }
}

pub fn parse_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
        .collect()
}

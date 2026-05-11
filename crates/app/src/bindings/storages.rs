use domain::{StorageDescriptor, StorageKind};
use slint::SharedString;

pub fn storage_to_row(d: &StorageDescriptor) -> crate::StorageRow {
    crate::StorageRow {
        id: SharedString::from(d.id.to_string()),
        name: SharedString::from(d.name.as_str()),
        kind: SharedString::from(kind_label(d.kind)),
        path: SharedString::from(config_display(&d.config_json, d.kind)),
        enabled: d.enabled,
    }
}

fn kind_label(kind: StorageKind) -> &'static str {
    match kind {
        StorageKind::Local  => "локальный",
        StorageKind::Smb    => "UNC/SMB",
        StorageKind::YaDisk => "Яндекс.Диск",
        StorageKind::GDrive => "Google Drive",
    }
}

pub fn kind_to_combo(kind: StorageKind) -> &'static str {
    match kind {
        StorageKind::Local  => "local",
        StorageKind::Smb    => "smb",
        StorageKind::YaDisk => "yadisk",
        StorageKind::GDrive => "gdrive",
    }
}

pub fn kind_from_combo(value: &str) -> StorageKind {
    match value {
        "smb"    => StorageKind::Smb,
        "yadisk" => StorageKind::YaDisk,
        "gdrive" => StorageKind::GDrive,
        _        => StorageKind::Local,
    }
}

fn config_display(config_json: &str, kind: StorageKind) -> String {
    let v: serde_json::Value = serde_json::from_str(config_json).unwrap_or_default();
    match kind {
        StorageKind::Local  => v["root"].as_str().unwrap_or("").to_owned(),
        StorageKind::Smb    => v["unc"].as_str().unwrap_or("").to_owned(),
        _                   => String::new(),
    }
}

pub fn path_to_config_json(kind: StorageKind, path: &str) -> String {
    match kind {
        StorageKind::Local  => format!("{{\"root\":{}}}", serde_json::to_string(path).unwrap_or_default()),
        StorageKind::Smb    => format!("{{\"unc\":{}}}", serde_json::to_string(path).unwrap_or_default()),
        _                   => "{}".to_owned(),
    }
}

pub fn path_from_config_json(config_json: &str, kind: StorageKind) -> String {
    let v: serde_json::Value = serde_json::from_str(config_json).unwrap_or_default();
    match kind {
        StorageKind::Local => v["root"].as_str().unwrap_or("").to_owned(),
        StorageKind::Smb   => v["unc"].as_str().unwrap_or("").to_owned(),
        _                  => String::new(),
    }
}

use std::rc::Rc;
use std::sync::Arc;

use application::commands;
use application::AppContext;
use domain::StorageDescriptor;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use tracing::{info, warn};
use uuid::Uuid;

use crate::bindings::storages::{
    kind_from_combo, kind_to_combo, path_from_config_json, path_to_config_json, storage_to_row,
};
use crate::bootstrap::make_storage;
use crate::AppWindow;

pub fn wire(window: &AppWindow, ctx: Arc<AppContext>) {
    window.set_storages_list(ModelRc::new(VecModel::<crate::StorageRow>::default()));
    refresh(window, ctx.clone());

    // «+ Добавить»
    {
        let weak = window.as_weak();
        window.on_add_storage(move || {
            let Some(w) = weak.upgrade() else { return };
            w.set_stor_dialog_id(SharedString::new());
            w.set_stor_dialog_name(SharedString::new());
            w.set_stor_dialog_kind(SharedString::from("local"));
            w.set_stor_dialog_path(SharedString::new());
            w.set_stor_dialog_enabled(true);
            w.set_stor_dialog_open(true);
        });
    }

    // Редактировать
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_edit_storage(move |id: SharedString| {
            let Ok(uuid) = Uuid::parse_str(id.as_str()) else {
                warn!(%id, "edit_storage: invalid uuid");
                return;
            };
            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let Ok(Some(desc)) = ctx.storages.get(uuid).await else {
                    warn!(%uuid, "edit_storage: not found");
                    return;
                };
                let kind_str = SharedString::from(kind_to_combo(desc.kind));
                let path = SharedString::from(path_from_config_json(&desc.config_json, desc.kind));
                let id_str = SharedString::from(desc.id.to_string());
                let name = SharedString::from(desc.name.as_str());
                let enabled = desc.enabled;
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        w.set_stor_dialog_id(id_str);
                        w.set_stor_dialog_name(name);
                        w.set_stor_dialog_kind(kind_str);
                        w.set_stor_dialog_path(path);
                        w.set_stor_dialog_enabled(enabled);
                        w.set_stor_dialog_open(true);
                    }
                });
            });
        });
    }

    // Удалить
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_delete_storage(move |id: SharedString| {
            let Ok(uuid) = Uuid::parse_str(id.as_str()) else {
                warn!(%id, "delete_storage: invalid uuid");
                return;
            };
            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                match commands::storages::delete(&ctx, uuid).await {
                    Ok(()) => {
                        ctx.storage_registry.remove(uuid);
                        info!(storage_id = %uuid, "storage deleted");
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = weak.upgrade() {
                                refresh_from_event_loop(&w, ctx);
                            }
                        });
                    }
                    Err(e) => warn!(error = %e, "delete storage failed"),
                }
            });
        });
    }

    // Browse path
    {
        let weak = window.as_weak();
        window.on_browse_storage_path(move || {
            let Some(w) = weak.upgrade() else { return };
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                w.set_stor_dialog_path(SharedString::from(path.to_string_lossy().as_ref()));
            }
        });
    }

    // Cancel
    {
        let weak = window.as_weak();
        window.on_cancel_storage_dialog(move || {
            if let Some(w) = weak.upgrade() {
                w.set_stor_dialog_id(SharedString::new());
                w.set_stor_dialog_open(false);
            }
        });
    }

    // Save
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_save_storage(move || {
            let Some(w) = weak.upgrade() else { return };
            let name = w.get_stor_dialog_name().to_string();
            let path = w.get_stor_dialog_path().to_string();
            if name.trim().is_empty() || path.trim().is_empty() {
                return;
            }
            let kind = kind_from_combo(w.get_stor_dialog_kind().as_str());
            let enabled = w.get_stor_dialog_enabled();
            let editing_id = w.get_stor_dialog_id().to_string();
            let config_json = path_to_config_json(kind, &path);

            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let desc = if editing_id.is_empty() {
                    StorageDescriptor {
                        id: Uuid::now_v7(),
                        kind,
                        name,
                        config_json,
                        secret_ref: None,
                        enabled,
                    }
                } else {
                    let Ok(edit_uuid) = Uuid::parse_str(&editing_id) else {
                        warn!(editing_id, "save_storage: invalid uuid");
                        return;
                    };
                    match ctx.storages.get(edit_uuid).await {
                        Ok(Some(existing)) => StorageDescriptor {
                            id: existing.id,
                            kind,
                            name,
                            config_json,
                            secret_ref: existing.secret_ref,
                            enabled,
                        },
                        _ => {
                            warn!(%edit_uuid, "save_storage edit: not found");
                            return;
                        }
                    }
                };

                match commands::storages::upsert(&ctx, &desc).await {
                    Ok(()) => {
                        info!(storage_id = %desc.id, "storage saved");
                        // Обновляем реестр.
                        if desc.enabled {
                            match make_storage(&desc) {
                                Ok(s) => ctx.storage_registry.register(desc.id, s),
                                Err(e) => warn!(error = %e, "failed to register storage"),
                            }
                        } else {
                            ctx.storage_registry.remove(desc.id);
                        }
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = weak.upgrade() {
                                w.set_stor_dialog_id(SharedString::new());
                                w.set_stor_dialog_open(false);
                                refresh_from_event_loop(&w, ctx);
                            }
                        });
                    }
                    Err(e) => warn!(error = %e, "save storage failed"),
                }
            });
        });
    }
}

fn refresh(window: &AppWindow, ctx: Arc<AppContext>) {
    let weak = window.as_weak();
    tokio::spawn(async move {
        load_and_apply(weak, ctx).await;
    });
}

fn refresh_from_event_loop(window: &AppWindow, ctx: Arc<AppContext>) {
    let weak: Weak<AppWindow> = window.as_weak();
    tokio::spawn(async move {
        load_and_apply(weak, ctx).await;
    });
}

async fn load_and_apply(weak: Weak<AppWindow>, ctx: Arc<AppContext>) {
    let storages = match ctx.storages.list().await {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "failed to load storages");
            return;
        }
    };
    let rows: Vec<crate::StorageRow> = storages.iter().map(storage_to_row).collect();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(w) = weak.upgrade() {
            w.set_storages_list(ModelRc::from(Rc::new(VecModel::from(rows))));
        }
    });
}

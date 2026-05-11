use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use application::commands;
use application::AppContext;
use domain::Source;
use time::OffsetDateTime;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel, Weak};
use tracing::{info, warn};

use crate::bindings::sources::{kind_from_combo, kind_to_combo, parse_tags, source_to_row};
use crate::AppWindow;

pub fn wire(window: &AppWindow, ctx: Arc<AppContext>) {
    window.set_sources(ModelRc::new(VecModel::<crate::SourceRow>::default()));
    refresh(window, ctx.clone());

    // «+ Добавить» — открыть диалог в чистом состоянии.
    {
        let weak = window.as_weak();
        window.on_add_source(move || {
            let Some(w) = weak.upgrade() else { return };
            w.set_src_dialog_id(SharedString::new());
            w.set_src_dialog_name(SharedString::new());
            w.set_src_dialog_path(SharedString::new());
            w.set_src_dialog_description(SharedString::new());
            w.set_src_dialog_tags(SharedString::new());
            w.set_src_dialog_kind(SharedString::from("folder"));
            w.set_src_dialog_open(true);
        });
    }

    // Редактирование — открыть диалог с заполненными полями.
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_edit_source(move |id: SharedString| {
            let Ok(uuid) = uuid::Uuid::parse_str(id.as_str()) else {
                warn!(%id, "edit_source: invalid uuid");
                return;
            };
            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let Ok(Some(src)) = ctx.sources.get(uuid).await else {
                    warn!(%uuid, "edit_source: not found");
                    return;
                };
                let src_id   = SharedString::from(src.id.to_string());
                let name     = SharedString::from(src.name.as_str());
                let path     = SharedString::from(src.path.to_string_lossy().as_ref());
                let desc     = SharedString::from(src.description.clone().unwrap_or_default());
                let tags     = SharedString::from(src.tags.join(", "));
                let kind     = SharedString::from(kind_to_combo(&src.kind));
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        w.set_src_dialog_id(src_id);
                        w.set_src_dialog_name(name);
                        w.set_src_dialog_path(path);
                        w.set_src_dialog_description(desc);
                        w.set_src_dialog_tags(tags);
                        w.set_src_dialog_kind(kind);
                        w.set_src_dialog_open(true);
                    }
                });
            });
        });
    }

    // Удаление.
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_delete_source(move |id: SharedString| {
            let Ok(uuid) = uuid::Uuid::parse_str(id.as_str()) else {
                warn!(id = %id, "delete_source: invalid uuid");
                return;
            };
            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                match commands::sources::delete(&ctx, uuid).await {
                    Ok(()) => {
                        info!(source_id = %uuid, "source deleted");
                        let weak = weak.clone();
                        let ctx2 = ctx.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = weak.upgrade() {
                                refresh_from_event_loop(&w, ctx2);
                            }
                        });
                    }
                    Err(e) => warn!(error = %e, "delete failed"),
                }
            });
        });
    }

    // Browse... — открыть нативный picker через rfd на UI-потоке.
    {
        let weak = window.as_weak();
        window.on_browse_source_path(move || {
            let Some(w) = weak.upgrade() else { return };
            // rfd::FileDialog блокирующий — приемлемо для модального диалога,
            // event loop встаёт на короткое время.
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                w.set_src_dialog_path(SharedString::from(path.to_string_lossy().as_ref()));
            }
        });
    }

    // Cancel — закрыть и сбросить id.
    {
        let weak = window.as_weak();
        window.on_cancel_source_dialog(move || {
            if let Some(w) = weak.upgrade() {
                w.set_src_dialog_id(SharedString::new());
                w.set_src_dialog_open(false);
            }
        });
    }

    // Save — собрать Source из полей, вызвать use-case.
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_save_source(move || {
            let Some(w) = weak.upgrade() else { return };
            let name = w.get_src_dialog_name().to_string();
            let path = w.get_src_dialog_path().to_string();
            if name.trim().is_empty() || path.trim().is_empty() {
                return;
            }
            let description = w.get_src_dialog_description().to_string();
            let tags = parse_tags(w.get_src_dialog_tags().as_str());
            let kind = kind_from_combo(w.get_src_dialog_kind().as_str());
            let editing_id = w.get_src_dialog_id().to_string();

            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let description = if description.trim().is_empty() { None } else { Some(description) };
                let now = OffsetDateTime::now_utc();

                let src = if editing_id.is_empty() {
                    let mut s = Source::new(kind, name, PathBuf::from(path));
                    s.description = description;
                    s.tags = tags;
                    s
                } else {
                    let Ok(edit_uuid) = uuid::Uuid::parse_str(&editing_id) else {
                        warn!(editing_id, "save_source: invalid uuid");
                        return;
                    };
                    match ctx.sources.get(edit_uuid).await {
                        Ok(Some(existing)) => Source {
                            id: existing.id,
                            created_at: existing.created_at,
                            updated_at: now,
                            kind,
                            name,
                            path: PathBuf::from(path),
                            description,
                            tags,
                            enabled: existing.enabled,
                        },
                        _ => {
                            warn!(%edit_uuid, "save_source edit: not found");
                            return;
                        }
                    }
                };

                match commands::sources::upsert(&ctx, &src).await {
                    Ok(()) => {
                        info!(source_id = %src.id, name = %src.name, "source saved");
                        let ctx2 = ctx.clone();
                        let weak = weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = weak.upgrade() {
                                w.set_src_dialog_id(SharedString::new());
                                w.set_src_dialog_open(false);
                                refresh_from_event_loop(&w, ctx2);
                            }
                        });
                    }
                    Err(e) => warn!(error = %e, "save source failed"),
                }
            });
        });
    }
}

fn refresh(window: &AppWindow, ctx: Arc<AppContext>) {
    let weak = window.as_weak();
    tokio::spawn(async move {
        let sources = match commands::sources::list(&ctx).await {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "failed to load sources");
                return;
            }
        };
        let rows: Vec<crate::SourceRow> = sources.iter().map(source_to_row).collect();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(w) = weak.upgrade() else { return };
            w.set_sources(ModelRc::from(Rc::new(VecModel::from(rows))));
        });
    });
}

/// Вариант обновления, который можно вызвать уже находясь внутри `invoke_from_event_loop`.
fn refresh_from_event_loop(window: &AppWindow, ctx: Arc<AppContext>) {
    let weak: Weak<AppWindow> = window.as_weak();
    tokio::spawn(async move {
        let sources = match commands::sources::list(&ctx).await {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "refresh: failed to load sources");
                return;
            }
        };
        let rows: Vec<crate::SourceRow> = sources.iter().map(source_to_row).collect();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(w) = weak.upgrade() {
                w.set_sources(ModelRc::from(Rc::new(VecModel::from(rows))));
            }
        });
    });
}

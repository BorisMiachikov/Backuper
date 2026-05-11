use std::sync::Arc;

use application::commands;
use application::AppContext;
use slint::{ComponentHandle, SharedString};
use tracing::{info, warn};

use crate::AppWindow;

pub fn wire(window: &AppWindow, ctx: Arc<AppContext>) {
    // Загружаем настройки при старте.
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        tokio::spawn(async move {
            let theme = load_str(&ctx, "theme", "system").await;
            let max_parallel = load_int(&ctx, "max_parallel", 2).await;
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(w) = weak.upgrade() {
                    w.set_settings_theme(SharedString::from(theme.as_str()));
                    w.set_color_scheme(SharedString::from(theme.as_str()));
                    w.set_settings_max_parallel(max_parallel);
                }
            });
        });
    }

    // Сохранение настроек.
    {
        let ctx = ctx.clone();
        let weak = window.as_weak();
        window.on_save_settings(move || {
            let Some(w) = weak.upgrade() else { return };
            let theme = w.get_settings_theme().to_string();
            let max_parallel = w.get_settings_max_parallel();
            let theme_clone = theme.clone();
            let ctx = ctx.clone();
            let weak = weak.clone();
            tokio::spawn(async move {
                let theme_json = serde_json::to_string(&theme_clone).unwrap_or_default();
                let parallel_json = serde_json::to_string(&max_parallel).unwrap_or_default();
                let r1 = commands::settings::set(&ctx, "theme", &theme_json).await;
                let r2 = commands::settings::set(&ctx, "max_parallel", &parallel_json).await;
                if r1.is_err() || r2.is_err() {
                    warn!("save settings failed");
                    return;
                }
                info!(theme = theme_clone, max_parallel, "settings saved");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(w) = weak.upgrade() {
                        w.set_color_scheme(SharedString::from(theme.as_str()));
                    }
                });
            });
        });
    }
}

async fn load_str(ctx: &AppContext, key: &str, default: &str) -> String {
    match commands::settings::get(ctx, key).await {
        Ok(Some(val)) => serde_json::from_str::<String>(&val).unwrap_or_else(|_| default.to_owned()),
        _ => default.to_owned(),
    }
}

async fn load_int(ctx: &AppContext, key: &str, default: i32) -> i32 {
    match commands::settings::get(ctx, key).await {
        Ok(Some(val)) => serde_json::from_str::<i32>(&val).unwrap_or(default),
        _ => default,
    }
}

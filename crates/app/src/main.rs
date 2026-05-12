#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod bindings;
mod bootstrap;
mod handlers;
mod notifications;
mod tray;

slint::include_modules!();

use std::sync::Arc;

use tracing::{error, info};

fn main() -> anyhow::Result<()> {
    // Стандартная файловая разметка %LOCALAPPDATA%\Backuper\…
    shared::paths::ensure_layout()?;
    let _log_guards = shared::logging::init(&shared::paths::logs_dir())?;
    info!("Backuper {} starting", env!("CARGO_PKG_VERSION"));

    // Один экземпляр приложения.
    let instance = single_instance::SingleInstance::new("ru.backuper.singleton")
        .map_err(|e| anyhow::anyhow!("single-instance: {e}"))?;
    if !instance.is_single() {
        info!("another instance is running, exiting");
        return Ok(());
    }

    // Tokio runtime для всей фоновой работы.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("backuper-worker")
        .build()?;

    // Композиция корня DI и запуск планировщика.
    let ctx = rt.block_on(bootstrap::build_context())?;
    let max_parallel = rt.block_on(bootstrap::read_max_parallel(&ctx));
    let scheduler = Arc::new(application::Scheduler::new(ctx.clone(), max_parallel));

    {
        let s = scheduler.clone();
        rt.spawn(async move { s.run().await });
    }

    // Удерживаем рантайм живым на время GUI.
    let _rt_guard = rt.enter();

    let window = AppWindow::new()?;
    handlers::wire_all(&window, ctx, scheduler.clone());

    // Закрытие окна — скрываем в трей, не завершаем процесс.
    window.window().on_close_requested(|| slint::CloseRequestResponse::HideWindow);

    // Системный трей.
    let tray_mgr = tray::TrayManager::new(window.as_weak(), scheduler)?;

    // Таймер для опроса событий трея (~10 раз в секунду).
    let poll_timer = slint::Timer::default();
    poll_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(100),
        move || {
            tray_mgr.poll();
        },
    );

    info!("entering Slint event loop");
    if let Err(e) = window.run() {
        error!(error = %e, "Slint event loop terminated with error");
    }
    Ok(())
}

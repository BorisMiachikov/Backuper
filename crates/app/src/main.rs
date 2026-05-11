#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod bindings;
mod bootstrap;
mod handlers;
mod notifications;

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
    let scheduler = Arc::new(application::Scheduler::new(ctx.clone(), 2));

    {
        let s = scheduler.clone();
        rt.spawn(async move { s.run().await });
    }

    // Удерживаем рантайм живым на время GUI: поток рантайма не блокируется,
    // главный поток отдаём Slint event loop'у.
    let _rt_guard = rt.enter();

    let window = AppWindow::new()?;
    handlers::wire_all(&window, ctx, scheduler);

    info!("entering Slint event loop");
    if let Err(e) = window.run() {
        error!(error = %e, "Slint event loop terminated with error");
    }
    Ok(())
}

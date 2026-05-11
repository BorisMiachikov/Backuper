use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

/// Инициализирует tracing: stderr + ротация суточных файлов в `logs_dir`.
///
/// Уровень управляется переменной окружения `RUST_LOG` (по умолчанию `info`).
/// Возвращает guard'ы — их нужно держать живыми до выхода из `main`.
pub fn init(logs_dir: &Path) -> std::io::Result<Vec<WorkerGuard>> {
    std::fs::create_dir_all(logs_dir)?;

    let file_appender = rolling::daily(logs_dir, "backuper.log");
    let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);
    let (stderr_writer, stderr_guard) = tracing_appender::non_blocking(std::io::stderr());

    let env = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(true)
        .json();

    let console_layer = fmt::layer()
        .with_writer(stderr_writer)
        .with_target(true)
        .compact();

    tracing_subscriber::registry()
        .with(env)
        .with(file_layer)
        .with(console_layer)
        .init();

    Ok(vec![file_guard, stderr_guard])
}

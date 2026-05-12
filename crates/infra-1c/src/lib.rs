//! Запуск 1cv8.exe / rac.exe для создания резервных копий баз 1С.
//!
//! `dump_file_base`   — выгрузка файловой ИБ через `1cv8.exe DESIGNER /IBPath ... /DumpIB ...`
//! `dump_server_base` — выгрузка серверной ИБ через `1cv8.exe DESIGNER /S server\base /DumpIB ...`
//! `terminate_sessions` — завершение сеансов через `rac.exe` (если настроен)
//! `run_hook`         — запуск произвольной команды с таймаутом

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use domain::onec::{CommandOutcome, HookOptions, OneCError, OneCFileBase, OneCRunner, OneCServerBase};
use secrecy::ExposeSecret;
use tokio::time::timeout;
use tracing::{debug, info, warn};

const DEFAULT_DUMP_TIMEOUT: Duration = Duration::from_secs(60 * 60); // 1 час

#[derive(Debug, Clone)]
pub struct OneCConfig {
    pub one_cv_8_exe: PathBuf,
    pub rac_exe: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct DefaultOneCRunner {
    pub config: OneCConfig,
}

impl DefaultOneCRunner {
    pub fn new(config: OneCConfig) -> Self {
        Self { config }
    }
}

/// Запустить исполняемый файл с аргументами и дождаться завершения.
async fn run_cmd(
    exe: &Path,
    args: Vec<String>,
    timeout_dur: Duration,
) -> Result<CommandOutcome, OneCError> {
    debug!(exe = %exe.display(), ?args, "spawn process");
    let mut cmd = tokio::process::Command::new(exe);
    for a in &args {
        cmd.arg(a);
    }

    let out = timeout(timeout_dur, cmd.output())
        .await
        .map_err(|_| OneCError::Timeout)?
        .map_err(OneCError::Io)?;

    let outcome = CommandOutcome {
        exit_code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    };
    info!(exit_code = outcome.exit_code, "process finished");

    if outcome.exit_code != 0 {
        return Err(OneCError::NonZeroExit(outcome.exit_code));
    }
    Ok(outcome)
}

#[async_trait]
impl OneCRunner for DefaultOneCRunner {
    /// Выгрузка файловой ИБ в `.dt` файл.
    async fn dump_file_base(
        &self,
        base: &OneCFileBase,
        out_dt: &Path,
    ) -> Result<CommandOutcome, OneCError> {
        if !self.config.one_cv_8_exe.exists() {
            return Err(OneCError::ExecutableMissing);
        }
        if let Some(parent) = out_dt.parent() {
            std::fs::create_dir_all(parent).map_err(OneCError::Io)?;
        }

        let mut args = vec![
            "DESIGNER".to_owned(),
            "/IBPath".to_owned(),
            base.path.to_string_lossy().into_owned(),
            "/DumpIB".to_owned(),
            out_dt.to_string_lossy().into_owned(),
            "/DisableStartupDialogs".to_owned(),
        ];

        if let Some(u) = &base.user {
            args.push("/N".to_owned());
            args.push(u.clone());
        }
        if let Some(p) = &base.password {
            args.push("/P".to_owned());
            args.push(p.expose_secret().to_owned());
        }

        run_cmd(&self.config.one_cv_8_exe, args, DEFAULT_DUMP_TIMEOUT).await
    }

    /// Выгрузка серверной ИБ в `.dt` файл.
    /// Использует `/UC` для получения монопольного доступа (1cv8 завершит сеансы сам).
    async fn dump_server_base(
        &self,
        base: &OneCServerBase,
        out_dt: &Path,
    ) -> Result<CommandOutcome, OneCError> {
        if !self.config.one_cv_8_exe.exists() {
            return Err(OneCError::ExecutableMissing);
        }
        if let Some(parent) = out_dt.parent() {
            std::fs::create_dir_all(parent).map_err(OneCError::Io)?;
        }

        // Формат: server:port\refbase  или  server\refbase
        let connection = match base.cluster_port {
            Some(p) => format!("{}:{}/{}", base.server, p, base.ref_base),
            None => format!("{}/{}", base.server, base.ref_base),
        };

        let mut args = vec![
            "DESIGNER".to_owned(),
            "/S".to_owned(),
            connection,
            "/DumpIB".to_owned(),
            out_dt.to_string_lossy().into_owned(),
            "/DisableStartupDialogs".to_owned(),
            // Запрашиваем монопольный доступ — 1cv8 дождётся освобождения сеансов.
            "/UC".to_owned(),
            "backuper_lock".to_owned(),
        ];

        if let Some(u) = &base.user {
            args.push("/N".to_owned());
            args.push(u.clone());
        }
        if let Some(p) = &base.password {
            args.push("/P".to_owned());
            args.push(p.expose_secret().to_owned());
        }

        run_cmd(&self.config.one_cv_8_exe, args, DEFAULT_DUMP_TIMEOUT).await
    }

    /// Принудительное завершение сеансов через `rac.exe`.
    /// Требует заполненного `rac_exe` в конфигурации.
    async fn terminate_sessions(&self, base: &OneCServerBase) -> Result<(), OneCError> {
        let rac = match &self.config.rac_exe {
            Some(p) => p.clone(),
            None => {
                return Err(OneCError::Other(
                    "rac_exe not configured; cannot terminate sessions".into(),
                ))
            }
        };

        if !rac.exists() {
            return Err(OneCError::Other(format!(
                "rac.exe not found: {}",
                rac.display()
            )));
        }

        // Шаг 1: получить UUID кластера.
        let server_addr = match base.cluster_port {
            Some(p) => format!("{}:{}", base.server, p),
            None => base.server.clone(),
        };

        let cluster_list = run_cmd(
            &rac,
            vec!["cluster".to_owned(), "list".to_owned(), server_addr.clone()],
            Duration::from_secs(30),
        )
        .await
        .map_err(|e| OneCError::Other(format!("rac cluster list: {e}")))?;

        let cluster_uuid = parse_rac_field(&cluster_list.stdout, "cluster")
            .ok_or_else(|| OneCError::Other("rac: cluster UUID not found".into()))?;

        // Шаг 2: завершить подключения к конкретной базе.
        let result = run_cmd(
            &rac,
            vec![
                "connection".to_owned(),
                "terminate".to_owned(),
                format!("--cluster={cluster_uuid}"),
                format!("--infobase={}", base.ref_base),
                server_addr,
            ],
            Duration::from_secs(30),
        )
        .await;

        match result {
            Ok(_) => {
                info!(ref_base = %base.ref_base, "sessions terminated");
                Ok(())
            }
            Err(OneCError::NonZeroExit(code)) => {
                // Код 1 — нет активных сеансов; это нормально.
                warn!(exit_code = code, "rac connection terminate returned non-zero (may be ok if no sessions)");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Запуск произвольного хука с таймаутом.
    async fn run_hook(
        &self,
        command: &str,
        opts: &HookOptions,
    ) -> Result<CommandOutcome, OneCError> {
        let timeout_dur = opts.timeout.unwrap_or(Duration::from_secs(300));

        // Разбиваем командную строку на токены.
        let mut parts = command.split_whitespace();
        let exe = parts.next().ok_or_else(|| OneCError::Other("empty hook command".into()))?;
        let args: Vec<String> = parts.map(str::to_owned).collect();

        let mut cmd = tokio::process::Command::new(exe);
        for a in &args {
            cmd.arg(a);
        }
        if let Some(dir) = &opts.working_dir {
            cmd.current_dir(dir);
        }

        debug!(command, "running hook");
        let out = timeout(timeout_dur, cmd.output())
            .await
            .map_err(|_| OneCError::Timeout)?
            .map_err(OneCError::Io)?;

        let outcome = CommandOutcome {
            exit_code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        };

        if outcome.exit_code != 0 {
            return Err(OneCError::NonZeroExit(outcome.exit_code));
        }
        Ok(outcome)
    }
}

/// Найти значение поля в выводе `rac.exe` формата `field : value`.
fn parse_rac_field(output: &str, field: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&format!("{field} : ")) {
            return Some(rest.trim().to_owned());
        }
        // rac печатает с отступами и дополнительными пробелами: "cluster             : <uuid>"
        if trimmed.starts_with(field) {
            if let Some(colon_pos) = trimmed.find(':') {
                let key = trimmed[..colon_pos].trim();
                if key == field {
                    return Some(trimmed[colon_pos + 1..].trim().to_owned());
                }
            }
        }
    }
    None
}

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use secrecy::SecretString;

#[derive(Debug, Clone)]
pub struct OneCFileBase {
    pub path: PathBuf,
    pub user: Option<String>,
    pub password: Option<SecretString>,
}

#[derive(Debug, Clone)]
pub struct OneCServerBase {
    pub server: String,
    pub cluster_port: Option<u16>,
    pub ref_base: String,
    pub user: Option<String>,
    pub password: Option<SecretString>,
}

#[derive(Debug, Clone, Default)]
pub struct HookOptions {
    pub timeout: Option<Duration>,
    pub working_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CommandOutcome {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, thiserror::Error)]
pub enum OneCError {
    #[error("1cv8.exe not found")]
    ExecutableMissing,
    #[error("base is locked / has active sessions")]
    BaseLocked,
    #[error("command failed: exit={0}")]
    NonZeroExit(i32),
    #[error("timeout")]
    Timeout,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("other: {0}")]
    Other(String),
}

#[async_trait]
pub trait OneCRunner: Send + Sync {
    async fn dump_file_base(
        &self,
        base: &OneCFileBase,
        out_dt: &Path,
    ) -> Result<CommandOutcome, OneCError>;

    async fn dump_server_base(
        &self,
        base: &OneCServerBase,
        out_dt: &Path,
    ) -> Result<CommandOutcome, OneCError>;

    async fn terminate_sessions(&self, base: &OneCServerBase) -> Result<(), OneCError>;

    async fn run_hook(
        &self,
        command: &str,
        opts: &HookOptions,
    ) -> Result<CommandOutcome, OneCError>;
}

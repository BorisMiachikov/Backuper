//! Запуск 1cv8.exe / rac. Stage 0 — заглушка, real-impl на Stage 3.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use domain::onec::{CommandOutcome, HookOptions, OneCError, OneCFileBase, OneCRunner, OneCServerBase};

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

#[async_trait]
impl OneCRunner for DefaultOneCRunner {
    async fn dump_file_base(
        &self,
        _base: &OneCFileBase,
        _out_dt: &Path,
    ) -> Result<CommandOutcome, OneCError> {
        Err(OneCError::Other("dump_file_base: not implemented (stage 0)".into()))
    }

    async fn dump_server_base(
        &self,
        _base: &OneCServerBase,
        _out_dt: &Path,
    ) -> Result<CommandOutcome, OneCError> {
        Err(OneCError::Other("dump_server_base: not implemented (stage 0)".into()))
    }

    async fn terminate_sessions(&self, _base: &OneCServerBase) -> Result<(), OneCError> {
        Err(OneCError::Other("terminate_sessions: not implemented (stage 0)".into()))
    }

    async fn run_hook(
        &self,
        _command: &str,
        _opts: &HookOptions,
    ) -> Result<CommandOutcome, OneCError> {
        Err(OneCError::Other("run_hook: not implemented (stage 0)".into()))
    }
}

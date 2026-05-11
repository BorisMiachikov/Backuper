use std::path::PathBuf;

use directories::ProjectDirs;
use once_cell::sync::OnceCell;

use crate::{APP_NAME, APP_ORG, APP_QUALIFIER};

static DIRS: OnceCell<ProjectDirs> = OnceCell::new();

fn dirs() -> &'static ProjectDirs {
    DIRS.get_or_init(|| {
        ProjectDirs::from(APP_QUALIFIER, APP_ORG, APP_NAME)
            .expect("could not resolve user directories")
    })
}

pub fn data_dir() -> PathBuf {
    dirs().data_local_dir().to_path_buf()
}

pub fn config_dir() -> PathBuf {
    dirs().config_dir().to_path_buf()
}

pub fn cache_dir() -> PathBuf {
    dirs().cache_dir().to_path_buf()
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn tmp_dir() -> PathBuf {
    data_dir().join("tmp")
}

pub fn app_db_path() -> PathBuf {
    data_dir().join("app.db")
}

pub fn vault_db_path() -> PathBuf {
    data_dir().join("vault.db")
}

pub fn vault_key_path() -> PathBuf {
    data_dir().join("vault.key")
}

pub fn ensure_layout() -> std::io::Result<()> {
    for d in [data_dir(), logs_dir(), tmp_dir(), config_dir(), cache_dir()] {
        std::fs::create_dir_all(d)?;
    }
    Ok(())
}

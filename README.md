# Backuper

Desktop-приложение на Rust для резервного копирования баз 1С (файловых и серверных) и произвольных каталогов.

- **GUI:** Slint 1.x (pure-Rust, без WebView).
- **Async:** tokio.
- **БД:** SQLite через SQLx (миграции в `/migrations`).
- **Архивация:** zip / 7z / zstd, AES-256.
- **Хранилища:** локальные, SMB/UNC, Яндекс.Диск, Google Drive.
- **Безопасность:** секреты через Windows DPAPI + AES-GCM, OAuth-токены изолированы.
- **Платформа:** Windows 10/11, заложен порт на Linux/macOS.

## Структура

```
crates/
  domain/                       чистые типы и трейты-порты
  application/                  use-cases, scheduler, pipeline
  infra-storage-sqlite/         SQLx-репозитории
  infra-secrets/                DPAPI / keyring
  infra-archive/                zip / 7z / zstd
  infra-1c/                     1cv8.exe / rac
  infra-cloud-yadisk/           Яндекс.Диск REST + OAuth
  infra-cloud-gdrive/           Google Drive REST + OAuth
  infra-fs/                     локальные диски, SMB
  shared/                       общие утилиты
  app/                          бинарь: Slint UI + composition root
migrations/                     SQLx миграции SQLite
installer/                      WiX/NSIS
docs/                           ADR и документация
```

См. архитектурный план в `C:\Users\lb426\.claude\plans\prompt-senior-shiny-finch.md`.

## Сборка

```powershell
cargo build --release -p app
```

## Запуск дев-сборки

```powershell
cargo run -p app
```

## Тесты

```powershell
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

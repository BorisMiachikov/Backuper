//! Use-cases (commands), вызываемые из UI.
//!
//! Каждая команда — функция, принимающая `&AppContext` и DTO; внутри валидация,
//! работа с репозиториями и публикация `DomainEvent`.

pub mod jobs;
pub mod settings;
pub mod sources;
pub mod storages;

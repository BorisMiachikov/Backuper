//! Маппинг доменных enum'ов в (discriminator, params_json) пары для SQLite.
//!
//! Договорённость: `kind` колонка хранит discriminator-строку (для индексов и
//! фильтров), `params_json` — JSON-объект с данными варианта (или `{}` для
//! unit-вариантов).

pub mod source_kind;

/// CloudLess API server — public library surface.
///
/// Exposes the router construction, AppEnv, and all port/adapter modules so
/// that a private hosted wrapper crate can depend on this via git tag and wire
/// in provider-specific adapters (e.g. Dodo billing) without modifying public code.
pub mod app_env;
pub mod config;
pub mod core;
pub mod infra;
pub mod web;

/// Embedded database migrations — re-exported so the private hosted binary can
/// run `MIGRATOR.run(&pool)` without maintaining a separate migrations directory.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

//! AppControl benchmark support crate.
//!
//! This crate exists so that the criterion benchmark files under `benches/`
//! can share helpers (synthetic DAG builders, SQLite seeding) without each
//! benchmark re-inventing them. The library itself is a thin facade — most of
//! the heavy lifting lives in the bench files directly.
//!
//! The crate is `publish = false` and is excluded from production builds: it
//! depends on `appcontrol-backend` with the `sqlite` feature so benches can
//! run against an in-memory database without dragging in the postgres driver.

pub mod dag_support;
pub mod permission_support;

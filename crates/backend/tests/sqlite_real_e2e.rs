//! SQLite Real E2E test suite -- launches actual backend, gateway, and agent binaries.
//!
//! Run with:
//!   CARGO_TARGET_DIR=$PWD/target-sqlite cargo test --no-default-features --features sqlite \
//!       --test sqlite_real_e2e --no-run
//! Then:
//!   CARGO_TARGET_DIR=$PWD/target-sqlite cargo test --no-default-features --features sqlite \
//!       --test sqlite_real_e2e -- --test-threads=1

#![cfg(all(feature = "sqlite", not(feature = "postgres")))]
#![allow(dead_code)]

#[path = "sqlite_real_e2e/harness.rs"]
mod harness;
#[path = "sqlite_real_e2e/test_start_stop.rs"]
mod test_start_stop;
#[path = "sqlite_real_e2e/test_switchover.rs"]
mod test_switchover;

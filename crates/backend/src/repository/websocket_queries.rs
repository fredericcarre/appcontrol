//! Query functions for websocket domain. All sqlx queries live here.

#![allow(unused_imports, dead_code)]
use crate::db::{DbPool, DbUuid, DbJson};
use serde_json::Value;
use uuid::Uuid;

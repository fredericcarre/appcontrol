//! CRUD endpoints for operation schedules (start/stop/restart automation).

pub mod types;
pub mod crud;
pub mod execution;
pub mod presets;

pub use types::*;
pub use crud::{
    list_app_schedules, create_app_schedule,
    list_component_schedules, create_component_schedule,
    get_schedule, update_schedule, delete_schedule,
};
pub use execution::{toggle_schedule, run_schedule_now, list_schedule_executions};
pub use presets::list_presets;

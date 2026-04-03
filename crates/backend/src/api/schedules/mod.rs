//! CRUD endpoints for operation schedules (start/stop/restart automation).

pub mod crud;
pub mod execution;
pub mod presets;
pub mod types;

pub use crud::{
    create_app_schedule, create_component_schedule, delete_schedule, get_schedule,
    list_app_schedules, list_component_schedules, update_schedule,
};
pub use execution::{list_schedule_executions, run_schedule_now, toggle_schedule};
pub use presets::list_presets;
pub use types::*;

//! Discovery API: passive topology scanning and multi-step DAG creation.
//!
//! ## Workflow
//! 1. **Collect**: `POST /trigger-all` or `/trigger/:agent_id`
//! 2. **Correlate**: `POST /correlate` — analyze reports
//! 3. **Create draft**: `POST /drafts`
//! 4. **Edit draft**: `PUT /drafts/:id/components` and `PUT /drafts/:id/dependencies`
//! 5. **Apply draft**: `POST /drafts/:id/apply` — creates a real application

pub mod trigger;
pub mod reports;
pub mod correlation;
pub mod draft;
pub mod enrichment;

// Re-export all public handler functions
pub use trigger::{trigger_scan, trigger_all};
pub use reports::{list_reports, get_report};
pub use correlation::correlate;
pub use draft::{
    list_drafts, get_draft, create_draft, update_draft_components,
    update_draft_dependencies, apply_draft,
    CreateDraftRequest, DraftComponentInput, DraftDependencyInput,
    UpdateComponentsRequest, UpdateComponentInput,
    UpdateDependenciesRequest, AddDependencyInput,
};
pub use enrichment::{
    list_schedules, create_schedule, update_schedule, delete_schedule,
    list_snapshots, compare_snapshots, read_file_content,
    CreateScheduleRequest, UpdateScheduleRequest,
    ListSnapshotsQuery, CompareSnapshotsRequest, ReadFileContentRequest,
};

//! Discovery API: passive topology scanning and multi-step DAG creation.
//!
//! ## Workflow
//! 1. **Collect**: `POST /trigger-all` or `/trigger/:agent_id`
//! 2. **Correlate**: `POST /correlate` — analyze reports
//! 3. **Create draft**: `POST /drafts`
//! 4. **Edit draft**: `PUT /drafts/:id/components` and `PUT /drafts/:id/dependencies`
//! 5. **Apply draft**: `POST /drafts/:id/apply` — creates a real application

pub mod correlation;
pub mod draft;
pub mod enrichment;
pub mod reports;
pub mod trigger;

// Re-export all public handler functions
pub use correlation::correlate;
pub use draft::{
    apply_draft, create_draft, get_draft, list_drafts, update_draft_components,
    update_draft_dependencies, AddDependencyInput, CreateDraftRequest, DraftComponentInput,
    DraftDependencyInput, UpdateComponentInput, UpdateComponentsRequest, UpdateDependenciesRequest,
};
pub use enrichment::{
    compare_snapshots, create_schedule, delete_schedule, list_schedules, list_snapshots,
    read_file_content, update_schedule, CompareSnapshotsRequest, CreateScheduleRequest,
    ListSnapshotsQuery, ReadFileContentRequest, UpdateScheduleRequest,
};
pub use reports::{get_report, list_reports};
pub use trigger::{trigger_all, trigger_scan};

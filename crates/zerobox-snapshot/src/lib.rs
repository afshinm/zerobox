//! Filesystem snapshot and rollback for zerobox sandboxes.
//!
//! Architecture inspired by [titor](https://github.com/winfunc/titor)

pub mod budget;
pub mod exclusion;
pub mod manager;
pub mod merkle;
pub mod store;
pub mod types;

pub use budget::WalkBudget;
pub use exclusion::{ExclusionConfig, ExclusionFilter, default_exclusions};
pub use manager::{SnapshotManager, compute_changes};
pub use store::ObjectStore;
pub use types::*;

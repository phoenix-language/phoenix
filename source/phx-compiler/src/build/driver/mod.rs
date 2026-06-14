//! `phx build` driver — artifacts under `build/`.
//!
//! ## Module map
//!
//! - [`package`] — project and dependency build orchestration
//! - [`incremental`] — manifest and `.pxi` freshness checks
//! - [`artifacts`] — `.pxi` / `.phx0` emission and manifest recording
//! - [`link_map`] — global function-id map and dependency link inputs
//! - [`util`] — I/O helpers and module-path utilities

mod artifacts;
mod incremental;
mod link_map;
mod package;
mod util;

/// Result of a successful project build.
#[derive(Debug, Clone)]
pub struct BuildResult {
    /// Path to linked output (`build/bin/` or `build/lib/`).
    pub output_path: std::path::PathBuf,
    /// Entry logical module path.
    pub entry_logical: String,
}

pub use package::{build_project, emit_interfaces_from_compiled, load_project_binary};

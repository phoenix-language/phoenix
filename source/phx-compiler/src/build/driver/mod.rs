//! `phx build` driver — orchestrates compile, emit, link, and incremental cache (M2).
//!
//! Owns the end-to-end project build pipeline after [`ProjectConfig`](crate::project::ProjectConfig)
//! is resolved: dependency prebuild, whole-program load/type-check, per-module artifact
//! emission, global function-id assignment, cross-crate link, and manifest update.
//!
//! ## Pipeline
//!
//! 1. Recursively build path dependencies (`build/deps/{name}/`).
//! 2. Load the workspace program; compare against `build/manifest.json`.
//! 3. On cache miss: resolve → type-check → (optional) lower/codegen per module.
//! 4. Emit `.pxi` interfaces; emit or reuse `.phx0` objects; link into `bin/` or `lib/`.
//! 5. Write updated manifest and return [`BuildResult`].
//!
//! ## Public entry points
//!
//! Re-exported at the [`crate::build`] root:
//!
//! - [`build_project`] — primary CLI/embedder build API
//! - [`emit_interfaces_from_compiled`] — interface-only path after external type-check
//! - [`load_project_binary`] / [`load_project_binary_with_options`] — decode linked bin
//!
//! ## Submodule map
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

use crate::compile::DiagnosticContext;
use phx_diagnostics::LintBag;

/// Result of a successful project build.
#[derive(Debug, Clone)]
pub struct BuildResult {
    /// Path to linked output (`build/bin/` or `build/lib/`).
    pub output_path: std::path::PathBuf,
    /// Entry logical module path.
    pub entry_logical: String,
    /// Lint warnings when type-check ran; empty on incremental cache hit.
    pub lints: LintBag,
    /// Formatting context for [`Self::lints`]; `None` when type-check was skipped.
    pub lint_context: Option<DiagnosticContext>,
}

pub use package::{
    build_project, emit_interfaces_from_compiled, load_project_binary,
    load_project_binary_with_options,
};

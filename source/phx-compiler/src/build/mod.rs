//! Project build driver, incremental manifest, and `build/` artifact layout (M2).
//!
//! Orchestrates `phx build`: path-dependency prebuild, whole-program load → resolve →
//! type-check, per-module `.pxi` / `.phx0` emission, cross-crate link, and manifest
//! recording for incremental rebuilds.
//!
//! ## Inputs and outputs
//!
//! - **Input:** [`ProjectConfig`](crate::project::ProjectConfig) from `phoenix.toml`,
//!   optional entry file override, and [`BuildOptions`].
//! - **Output:** [`BuildResult`] with linked `build/bin/` or `build/lib/` path, entry
//!   logical module, and lint bag when type-check ran.
//!
//! ## Artifact tree
//!
//! Under the project `build.dir` (default `build/`):
//!
//! - `manifest.json` — per-module source/`.pxi` hashes and artifact paths
//! - `pxi/` — workspace interface files (`.pxi`)
//! - `phx0/` — per-module object bytecode
//! - `bin/` or `lib/` — linked PHX0 image
//! - `deps/{name}/` — prebuilt path-dependency artifacts
//!
//! ## Public entry points
//!
//! - [`build_project`] — full build (dependencies first, then workspace link)
//! - [`emit_interfaces_from_compiled`] — write `.pxi` + manifest after an external type-check
//! - [`load_project_binary`] / [`load_project_binary_with_options`] — load linked bin for `phx run`
//!
//! ## Submodule map
//!
//! - [`driver`] — build orchestration (package, incremental, artifacts, link map)
//! - [`manifest`] — `build/manifest.json` parse/write and staleness checks
//! - [`options`] — [`BuildOptions`] and [`LoadOptions`]
//! - [`error`] — [`BuildError`]

mod driver;
mod error;
mod manifest;
mod options;

pub use driver::{
    BuildResult, build_project, emit_interfaces_from_compiled, load_project_binary,
    load_project_binary_with_options,
};
pub use error::BuildError;
pub use options::{BuildOptions, LoadOptions};

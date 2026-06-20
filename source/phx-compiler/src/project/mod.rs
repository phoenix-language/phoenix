//! Phoenix project configuration (`phoenix.toml`), discovery, and `build/` layout (M2).
//!
//! Resolves workspace metadata before [`crate::build`] runs: locate `phoenix.toml`, parse
//! package type and path dependencies, inject the bundled [`std`](stdlib) library when enabled,
//! and compute artifact paths under `build.dir`.
//!
//! ## Inputs and outputs
//!
//! - **Input:** A filesystem path (CLI cwd, entry file, or explicit `--project` root).
//! - **Output:** [`ProjectConfig`] — validated `phoenix.toml` plus absolute roots for
//!   `module_src` and `build.dir`; [`BuildLayout`] helpers for per-module `.pxi` / `.phx0` paths.
//!
//! ## Public entry points
//!
//! - [`discover_project`] / [`resolve_project`] — find and load `phoenix.toml`
//! - [`ProjectConfig::load`] — parse and validate a known project root
//! - [`BuildLayout::new`] — artifact path helpers for the workspace `build/` tree
//!
//! ## Submodule map
//!
//! - [`config`] — `phoenix.toml` schema, [`ProjectConfig`], [`ProjectError`]
//! - [`discover`] — upward walk to locate the nearest project marker
//! - [`layout`] — `build/pxi`, `build/phx0`, `build/deps/{name}/` path helpers
//! - [`stdlib`] — bundled `std` package discovery and dependency injection

mod config;
mod discover;
mod layout;
mod stdlib;

pub use config::{PackageType, ProjectConfig, ProjectError};
pub use discover::{discover_project, resolve_project};
pub use layout::BuildLayout;

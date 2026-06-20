//! Subcommand handlers dispatched from [`crate::run`].
//!
//! Each public `run_*` function is the entry point for one `phx` subcommand.
//! [`crate::run`] parses global flags and routes [`crate::args::Command`]
//! variants here; handlers resolve workflow mode, drive the compiler or VM,
//! and return a [`crate::exit::CliExit`].
//!
//! ```text
//! parse_env_args ──► dispatch (lib.rs)
//!                         │
//!         ┌───────────────┼───────────────┬──────────────┐
//!         ▼               ▼               ▼              ▼
//!   run_check       run_build       run_compile      run_run
//!         │               │               │              │
//!         └───────────────┴───────────────┴──────────────┘
//!                         │
//!              Reporter + workflow + lints
//!                         │
//!                         ▼
//!                     CliExit
//! ```
//!
//! Handlers share [`crate::report::Reporter`] for stderr diagnostics,
//! [`crate::workflow`] for project vs standalone resolution, and
//! [`crate::lints`] where compilation runs a lint pass. Implementation lives
//! in private submodules (`check`, `build`, `compile`, `run`, `explain`); this
//! module re-exports only the dispatch entry points.
//!
//! ## Public entry points
//!
//! - [`run_check`] — type-check without codegen.
//! - [`run_build`] — compile a project to bytecode artifacts.
//! - [`run_compile`] — compile a single file or project entry to `.phx0`.
//! - [`run_run`] — compile (when needed) and execute on the VM.
//! - [`run_explain`] — print documentation for a diagnostic error code.

mod build;
mod check;
mod compile;
mod explain;
mod run;

/// Entry point for `phx build`.
pub use build::run_build;
/// Entry point for `phx check`.
pub use check::run_check;
/// Entry point for `phx compile`.
pub use compile::run_compile;
/// Entry point for `phx explain <code>`.
pub use explain::run_explain;
/// Entry point for `phx run`.
pub use run::run_run;

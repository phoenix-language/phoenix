//! Multi-file module loading, import graph, and cross-module resolve.
//!
//! ## Pass role
//!
//! Sits between parse and [`crate::resolver`]: discovers every `.phx` file reachable from the
//! compile entry, builds a shared [`Interner`], validates filesystem layout for project builds,
//! and merges modules into one [`ResolvedProgram`] for [`crate::typeck`].
//!
//! ## Pipeline
//!
//! 1. [`load_program`] / [`load_program_with_context`] — BFS load, parse, `#import` and `mod`
//!    expansion, import-edge collection, cycle check (with `.pxi` SCC escape)
//! 2. [`resolve_loaded_program`] — per-module def collection, `#import` binding, prelude injection,
//!    then body resolve via [`crate::resolver`]
//!
//! ## Entry points
//!
//! - [`load_program`] — single-package fallback (`--module-src` / tests)
//! - [`load_program_with_context`] — workspace + path dependencies from [`ProgramLoadContext`]
//! - [`resolve_loaded_program`] — name resolution after a successful load
//!
//! Submodule filesystem rules and `mod`/`pub mod` visibility live in [`discover`]; logical path
//! mapping in [`path`]; import SCC ordering in [`graph`].

mod discover;
mod graph;
pub(crate) mod import_resolve;
mod load_context;
mod loader;
mod path;
mod prelude;
mod resolve_loaded_program;
mod source_text;

pub use discover::SubmoduleRegistry;
pub use graph::import_target_module;
pub use load_context::ProgramLoadContext;
pub use loader::{LoadedModule, LoadedProgram, ModuleId, load_program, load_program_with_context};
pub use path::ModulePath;
pub use resolve_loaded_program::resolve_loaded_program;
pub use source_text::SourceText;

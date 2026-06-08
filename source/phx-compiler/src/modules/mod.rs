//! Multi-file module loading, import graph, and crate assembly.

mod graph;
pub(crate) mod import_resolve;
#[allow(dead_code)]
// incremental `.pxi`-only import surface; wired via exports_for_dependency today
mod interface_loader;
mod load_context;
mod loader;
mod path;
mod prelude;
mod resolve_loaded_program;
mod source_text;

pub use graph::import_target_module;
pub use load_context::ProgramLoadContext;
pub use loader::{LoadedModule, LoadedProgram, ModuleId, load_program, load_program_with_context};
pub use path::ModulePath;
pub use resolve_loaded_program::resolve_loaded_program;
pub use source_text::SourceText;

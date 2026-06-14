//! Multi-file module loading, import graph, and crate assembly.

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

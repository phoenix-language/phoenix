//! Multi-file module loading, import graph, and crate assembly.

mod graph;
#[allow(dead_code)]
// incremental `.pxi`-only import surface; wired via exports_for_dependency today
mod interface_loader;
mod load_context;
mod loader;
mod path;
mod resolve_crate;

pub use graph::import_target_module;
pub use load_context::CrateLoadContext;
pub use loader::{LoadedCrate, LoadedModule, ModuleId, load_crate, load_crate_with_context};
pub use path::ModulePath;
pub use resolve_crate::resolve_crate;

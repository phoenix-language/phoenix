//! Multi-file module loading, import graph, and crate assembly.

mod graph;
#[allow(dead_code)] // M2: `.pxi`-only import resolution
mod interface_loader;
mod loader;
mod path;
mod resolve_crate;

pub use graph::import_target_module;
pub use loader::{LoadedCrate, LoadedModule, ModuleId, load_crate, load_crate_with_layout};
pub use path::ModulePath;
pub use resolve_crate::resolve_crate;

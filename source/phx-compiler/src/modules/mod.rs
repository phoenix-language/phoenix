//! Multi-file module loading, import graph, and crate assembly.

mod graph;
mod interface_loader;
mod loader;
mod path;
mod resolve_crate;

pub use graph::import_target_module;
pub use interface_loader::{bindings_from_pxi, InterfaceLoadError, PxiBinding};
pub use loader::{LoadedCrate, LoadedModule, ModuleId, load_crate, load_crate_with_layout};
pub use path::ModulePath;
pub use resolve_crate::resolve_crate;

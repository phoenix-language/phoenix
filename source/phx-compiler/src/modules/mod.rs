//! Multi-file module loading, import graph, and crate assembly.

mod graph;
mod loader;
mod path;
mod resolve_crate;

pub use loader::{LoadedCrate, LoadedModule, ModuleId, load_crate};
pub use path::ModulePath;
pub use resolve_crate::resolve_crate;

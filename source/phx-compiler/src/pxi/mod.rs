//! Per-module `.pxi` interface files (M2).

mod emit;
mod format;
mod hash;

pub use emit::{build_pxi_for_module, module_dependencies};
pub use format::{PxiDependency, PxiError, PxiExport, PxiFile, def_kind_to_pxi};
pub use hash::{digest_bytes, digest_file};

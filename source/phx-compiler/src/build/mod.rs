//! Project build driver and manifest (M2).

mod driver;
mod error;
mod manifest;
mod options;

pub use driver::{
    BuildResult, build_project, emit_interfaces_from_compiled, load_project_binary,
    load_project_binary_with_options,
};
pub use error::BuildError;
pub use options::{BuildOptions, LoadOptions};

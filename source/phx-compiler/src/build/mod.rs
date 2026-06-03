//! Project build driver and manifest (M2).

mod driver;
mod error;
mod manifest;

pub use driver::{build_project, load_project_binary, BuildResult};
pub use error::BuildError;

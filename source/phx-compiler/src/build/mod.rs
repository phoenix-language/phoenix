//! Project build driver and manifest (M2).

mod driver;
mod error;
mod manifest;

pub use driver::{BuildResult, build_project, load_project_binary};
pub use error::BuildError;

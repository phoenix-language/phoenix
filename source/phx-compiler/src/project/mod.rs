//! Phoenix project configuration (`phoenix.toml`) and build layout.

mod config;
mod discover;
mod layout;

pub use config::{ProjectConfig, ProjectError};
pub use discover::{discover_project, resolve_project};
pub use layout::{BuildLayout, ModuleArtifacts};

//! Subcommand handlers.

mod build;
mod check;
mod compile;
mod explain;
mod run;

pub use build::run_build;
pub use check::run_check;
pub use compile::run_compile;
pub use explain::run_explain;
pub use run::run_run;

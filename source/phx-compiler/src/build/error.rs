//! Build driver errors (M2).

use std::path::PathBuf;

use crate::project::ProjectError;
use crate::pxi::PxiError;
use phx_diagnostics::{DiagnosticBag, LowerBag, TypeCheckBag};
use phx_syntax::ParseBag;

/// Failure during `phx build`.
#[derive(Debug)]
pub enum BuildError {
    /// Project configuration.
    Project(ProjectError),
    /// Parse failure.
    Parse(ParseBag),
    /// Resolve failure.
    Resolve(DiagnosticBag),
    /// Type-check failure.
    TypeCheck(TypeCheckBag),
    /// IR lowering failure.
    Lower(LowerBag),
    /// `.pxi` failure.
    Pxi(PxiError),
    /// Linker failure.
    Link(crate::link::LinkError),
    /// Stale or missing interface.
    StaleInterface {
        /// Module path.
        module: String,
        /// Reason.
        message: String,
    },
    /// Export does not match `.pxi`.
    InterfaceMismatch {
        /// Module path.
        module: String,
        /// Symbol name.
        name: String,
        /// Detail.
        message: String,
    },
    /// I/O error.
    Io {
        /// Path.
        path: PathBuf,
        /// Message.
        message: String,
    },
}

impl BuildError {
    /// User-facing message.
    #[must_use]
    pub fn to_message(&self) -> String {
        match self {
            Self::Project(e) => e.to_string(),
            Self::Parse(e) => e.to_string(),
            Self::Resolve(b) => b.to_string(),
            Self::TypeCheck(b) => b.to_string(),
            Self::Lower(b) => b.to_string(),
            Self::Pxi(e) => e.to_string(),
            Self::Link(e) => e.to_string(),
            Self::StaleInterface { module, message } => {
                format!("stale interface for module `{module}`: {message}")
            }
            Self::InterfaceMismatch {
                module,
                name,
                message,
            } => {
                format!("interface mismatch in `{module}` for `{name}`: {message}")
            }
            Self::Io { path, message } => format!("I/O error at {}: {message}", path.display()),
        }
    }
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_message())
    }
}

impl std::error::Error for BuildError {}

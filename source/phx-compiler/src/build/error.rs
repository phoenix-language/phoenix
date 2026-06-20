//! Build driver errors for `phx build`, link, and artifact I/O (M2).
//!
//! [`BuildError`] is the single failure type for the project build driver: configuration
//! load, whole-program compile passes, `.pxi` / `.phx0` emission, cross-crate link,
//! PHX0 encode/decode, incremental interface staleness, and filesystem operations.
//!
//! The CLI and embedders map these to user-facing diagnostics via [`BuildError::to_message`];
//! [`std::fmt::Display`] and [`std::error::Error`] delegate to the same formatting.
//!
//! ## Error taxonomy
//!
//! Variants mirror pipeline stages in roughly execution order:
//!
//! 1. **Configuration** — [`BuildError::Project`]
//! 2. **Front-end passes** — [`BuildError::Parse`], [`BuildError::Resolve`],
//!    [`BuildError::TypeCheck`]
//! 3. **Back-end passes** — [`BuildError::Lower`], [`BuildError::IrValidate`],
//!    [`BuildError::Codegen`], [`BuildError::Encode`], [`BuildError::Verify`]
//! 4. **Cross-crate artifacts** — [`BuildError::Pxi`], [`BuildError::Link`],
//!    [`BuildError::StaleInterface`], [`BuildError::InterfaceMismatch`]
//! 5. **Filesystem** — [`BuildError::Io`]
//!
//! Pass-stage variants wrap the corresponding diagnostic bag or domain error from the
//! underlying crate; incremental and interface variants carry module path and detail strings
//! for embedders that do not need full diagnostic rendering.

use std::path::PathBuf;

use crate::project::ProjectError;
use crate::pxi::PxiError;
use phx_diagnostics::{DiagnosticBag, IrBag, LowerBag, TypeCheckBag};
use phx_syntax::ParseBag;

/// Failure during `phx build`, link, or binary load.
///
/// Returned by [`super::build_project`], [`super::emit_interfaces_from_compiled`], and
/// [`super::load_project_binary_with_options`]. Each variant corresponds to a specific
/// pipeline stage or I/O operation; use [`BuildError::to_message`] for a single-line
/// user-facing summary.
#[derive(Debug)]
pub enum BuildError {
    /// Project configuration could not be loaded or is invalid.
    ///
    /// Wraps [`ProjectError`] from `phoenix.toml` parsing, path resolution, or layout
    /// validation (missing module root, bad dependency path, invalid package type).
    Project(ProjectError),

    /// Source failed to parse.
    ///
    /// Wraps a [`ParseBag`] with one or more syntax diagnostics. Emitted during program
    /// load or when re-parsing changed modules during incremental rebuild.
    Parse(ParseBag),

    /// Name resolution failed after a successful parse.
    ///
    /// Wraps a [`DiagnosticBag`] from the resolver (unresolved imports, duplicate
    /// definitions, invalid module paths).
    Resolve(DiagnosticBag),

    /// Type checking failed after successful resolution.
    ///
    /// Wraps a [`TypeCheckBag`] (ownership violations, type mismatches, unhandled
    /// `Result`/`Option`, trait bound failures).
    TypeCheck(TypeCheckBag),

    /// IR lowering failed after a successful type-check.
    ///
    /// Wraps a [`LowerBag`] from AST → IR translation (unsupported constructs, internal
    /// lowering invariants).
    Lower(LowerBag),

    /// IR validation failed (debug builds or `PHX_VALIDATE_IR=1`).
    ///
    /// Wraps an [`IrBag`] from the optional post-lower IR checker. Not run in release
    /// builds unless the environment variable is set.
    IrValidate(IrBag),

    /// `.pxi` interface file read, write, or parse failed.
    ///
    /// Wraps [`PxiError`] (checksum mismatch, malformed export table, I/O at the `.pxi`
    /// path). Also used when dependency interface files are missing or corrupt during
    /// link-map construction.
    Pxi(PxiError),

    /// Cross-crate link failed.
    ///
    /// Wraps [`crate::link::LinkError`] (duplicate symbols, unresolved imports across
    /// workspace and path-dependency objects, incompatible calling conventions).
    Link(crate::link::LinkError),

    /// Per-module bytecode codegen failed.
    ///
    /// Wraps [`crate::codegen::CodegenError`] from IR → PHX0 object emission.
    Codegen(crate::codegen::CodegenError),

    /// PHX0 object encode failed.
    ///
    /// Wraps [`phx_bytecode::EncodeError`] when serializing a [`phx_bytecode::BytecodeModule`]
    /// to disk (section size overflow, invalid metadata).
    Encode(phx_bytecode::EncodeError),

    /// Bytecode verifier rejected a module.
    ///
    /// Wraps [`phx_bytecode::VerifyError`] from structural checks on decoded bytecode
    /// (invalid jumps, stack underflow, out-of-bounds section references). Emitted during
    /// compile-time verify or when [`super::LoadOptions::verify_on_load`] is enabled.
    Verify(phx_bytecode::VerifyError),

    /// Incremental build found a stale or missing interface for a module.
    ///
    /// Returned when `build/manifest.json` or a dependency `.pxi` does not match live
    /// source hashes, or when a required prebuilt artifact is absent. The driver may
    /// surface this before recompilation or when a path dependency was not built.
    StaleInterface {
        /// Logical module path (e.g. `my_crate::foo::bar`).
        module: String,
        /// Human-readable reason (missing manifest entry, hash mismatch, absent file).
        message: String,
    },

    /// A workspace export does not match its recorded `.pxi` interface.
    ///
    /// Returned during link-map construction when a symbol's mangled name, arity, or
    /// type signature differs from the dependency interface the linker expects.
    InterfaceMismatch {
        /// Logical module path that owns the export.
        module: String,
        /// Export symbol name (mangled or logical, depending on context).
        name: String,
        /// Detail describing the mismatch (signature delta, missing export).
        message: String,
    },

    /// Filesystem read, write, or directory operation failed.
    ///
    /// Carries the affected path when known; an empty path indicates a bare
    /// [`std::io::Error`] without file context (see driver I/O helpers in
    /// [`super::driver::util`]).
    Io {
        /// Path associated with the failure; empty when no path was available.
        path: PathBuf,
        /// Underlying OS error message.
        message: String,
    },
}

impl BuildError {
    /// Formats a single-line user-facing message for this error.
    ///
    /// Pass-stage variants delegate to the wrapped bag or domain error's [`Display`]
    /// implementation. Incremental and interface variants include the module path;
    /// I/O variants include the path when non-empty.
    ///
    /// Prefer this over pattern-matching when the caller only needs a string for logging
    /// or CLI output; use the enum variants directly when structured diagnostics are
    /// required.
    #[must_use]
    pub fn to_message(&self) -> String {
        match self {
            Self::Project(e) => e.to_string(),
            Self::Parse(e) => e.to_string(),
            Self::Resolve(b) => b.to_string(),
            Self::TypeCheck(b) => b.to_string(),
            Self::Lower(b) => b.to_string(),
            Self::IrValidate(b) => b.to_string(),
            Self::Pxi(e) => e.to_string(),
            Self::Link(e) => e.to_string(),
            Self::Codegen(e) => e.to_string(),
            Self::Encode(e) => e.to_string(),
            Self::Verify(e) => e.to_string(),
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

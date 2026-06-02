//! Compilation unit after parse and resolve.
//!
//! [`CompilationUnit::source`] is an owned [`String`] so the unit can outlive file I/O buffers and
//! match diagnostic spans when reporting errors from disk.

use std::path::PathBuf;

use crate::resolver::ResolvedProgram;

/// A fully parsed and resolved Phoenix source file.
#[derive(Debug, Clone, PartialEq)]
pub struct CompilationUnit {
    /// Source path when loaded from disk.
    pub path: Option<PathBuf>,
    /// Owned copy of the Phoenix source (required after `read_to_string` / for stable lifetimes).
    pub source: String,
    /// Resolved program and definitions.
    pub resolved: ResolvedProgram,
}

//! Shared immutable source text for one `.phx` file.
//!
//! ## Pass role
//!
//! Holds the raw UTF-8 source for a single module after load. [`SourceText`] is stored on
//! [`super::loader::LoadedModule`] and copied into [`crate::resolver::SourceModule`] so diagnostics,
//! span rendering, and later passes can reference source without cloning large strings on every
//! module handle.
//!
//! ## Design
//!
//! The type is [`Arc<str>`]: cheap `Clone` for shared ownership, immutable bytes, and no per-clone
//! heap copy of the file contents. Load assigns one [`SourceText`] per parsed module; resolve and
//! typeck borrow it through module tables rather than duplicating `String` buffers.

use std::sync::Arc;

/// Shared, immutable source text for one `.phx` file.
///
/// Created during [`super::loader::load_program_with_context`] when a module file is read from disk.
/// Cloning this handle is O(1); the underlying text is shared until the last owner is dropped.
pub type SourceText = Arc<str>;

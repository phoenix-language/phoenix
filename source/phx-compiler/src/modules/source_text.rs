//! Shared immutable source text for one `.phx` file.
//!
//! [`SourceText`] is stored on [`super::loader::LoadedModule`] and copied into
//! [`crate::resolver::SourceModule`] so diagnostics and later passes can reference source without
//! duplicating large strings per clone.

use std::sync::Arc;

/// Shared, immutable source text for one `.phx` file (`Arc<str>`).
pub type SourceText = Arc<str>;

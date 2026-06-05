//! Shared immutable source text for one `.phx` file.

use std::sync::Arc;

/// Shared, immutable source text for one `.phx` file.
pub type SourceText = Arc<str>;

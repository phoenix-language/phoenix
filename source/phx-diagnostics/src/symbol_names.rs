//! Symbol name resolution for human-readable diagnostics.

/// Resolves interned symbol indices to spellings (implemented by the compiler `Interner`).
pub trait SymbolNames {
    /// Returns the text for `symbol_index` when it is a valid interned spelling.
    fn symbol_name(&self, symbol_index: u32) -> Option<&str>;
}

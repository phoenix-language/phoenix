//! Proof token that a [`BytecodeModule`] passed verification.

use super::module::BytecodeModule;

/// Proof that a module passed [`super::verify`].
///
/// Constructible only through the verifier — production VM entry points require this token.
#[derive(Debug, Clone, Copy)]
pub struct VerifiedModule<'a> {
    module: &'a BytecodeModule,
}

impl<'a> VerifiedModule<'a> {
    /// Returns the verified bytecode image.
    #[must_use]
    pub fn module(&self) -> &'a BytecodeModule {
        self.module
    }

    pub(crate) fn new(module: &'a BytecodeModule) -> Self {
        Self { module }
    }
}

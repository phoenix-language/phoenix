//! Proof token that a [`BytecodeModule`] passed static verification.
//!
//! [`VerifiedModule`] is the type-system boundary for the verify-before-execute invariant (the
//! Wasm model: validate once at load, execute trusting static checks). Production VM entry points
//! such as [`phx_vm::run`](phx_vm::run) accept only this token, not a raw [`BytecodeModule`].
//!
//! ## Proof token pattern
//!
//! The token is a zero-cost wrapper around `&BytecodeModule` with a private constructor. Only
//! [`crate::verify`] can construct it after [`verify`](crate::verify) succeeds, so callers cannot
//! forge verified status without running the full verifier. Decode-only paths
//! ([`BytecodeModule::decode`]) do not produce a token; embedders must call [`verify`] on every
//! image before execution.
//!
//! ## Relationship to [`crate::verify`]
//!
//! [`verify`](crate::verify) runs header/section checks, entry-function validation, optional PC
//! span checks, and per-function CFG stack analysis. On success it returns
//! [`VerifiedModule::new`] — the only construction site in the crate. On failure it returns
//! [`VerifyError`](crate::VerifyError) and no token is issued.
//!
//! ## Relationship to the VM
//!
//! The interpreter assumes verifier-proven invariants (valid jump targets, consistent stack depth
//! along all paths, operand indices in range) and keeps only dynamic checks the static pass cannot
//! prove (runtime division by zero, heap cap, bounds). See
//! `phx_vm::interpreter` for the verified vs `#[doc(hidden)]` unverified entry points.
//!
//! ## In this module
//!
//! - [`VerifiedModule`] — opaque proof of successful verification.
//! - [`VerifiedModule::module`] — borrow the underlying image for execution or inspection.

use super::module::BytecodeModule;

/// Opaque proof that `module` passed [`crate::verify`].
///
/// Wraps a shared reference to the verified [`BytecodeModule`]. The lifetime `'a` ties the token
/// to that borrow so the VM cannot outlive the image it was verified against.
///
/// Obtain only from [`crate::verify`]; do not construct manually. Mutation tests that need to
/// bypass verification use `#[doc(hidden)]` unverified VM APIs instead.
///
/// # Examples
///
/// ```
/// use phx_bytecode::{BytecodeModule, verify};
///
/// let module = BytecodeModule::empty();
/// // Empty modules fail verification — no token is issued.
/// assert!(verify(&module).is_err());
/// ```
#[derive(Debug, Clone, Copy)]
pub struct VerifiedModule<'a> {
    module: &'a BytecodeModule,
}

impl<'a> VerifiedModule<'a> {
    /// Returns the verified bytecode image.
    ///
    /// Safe to pass to [`phx_vm::run`](phx_vm::run) and related production entry points. The
    /// reference is the same module that was checked by [`crate::verify`].
    #[must_use]
    pub fn module(&self) -> &'a BytecodeModule {
        self.module
    }

    /// Constructs a proof token after [`crate::verify`] succeeds.
    ///
    /// # Panics
    ///
    /// Never panics.
    pub(crate) fn new(module: &'a BytecodeModule) -> Self {
        Self { module }
    }
}

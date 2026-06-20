//! Build and load options for the project driver (M2).
//!
//! [`BuildOptions`] controls incremental behavior and interface-only emission for
//! [`super::build_project`]. [`LoadOptions`] controls optional bytecode verification when
//! loading a linked binary via [`super::load_project_binary_with_options`].

/// Controls optional bytecode verification when loading a linked binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoadOptions {
    /// When `true`, run the bytecode verifier after decode (defense in depth at load).
    pub verify_on_load: bool,
}

/// Controls incremental rebuild and interface-only emission for [`super::build_project`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuildOptions {
    /// Rebuild all modules regardless of manifest staleness.
    pub force: bool,
    /// Emit `.pxi` and `manifest.json` only; skip per-module `.phx0` codegen and link.
    pub emit_interface_only: bool,
}

impl BuildOptions {
    /// Convenience constructor for forced full builds.
    #[must_use]
    pub const fn force(force: bool) -> Self {
        Self {
            force,
            emit_interface_only: false,
        }
    }
}

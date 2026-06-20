//! Options for [`super::driver::build_project`] and [`super::driver::load_project_binary`].

/// Controls optional bytecode verification when loading a linked binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoadOptions {
    /// When `true`, run the bytecode verifier after decode (defense in depth at load).
    pub verify_on_load: bool,
}

/// Controls incremental and interface-only project builds.
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

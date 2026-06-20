//! Build and load options for the project driver (M2).
//!
//! [`BuildOptions`] controls incremental rebuild behavior and interface-only emission for
//! [`super::build_project`] and [`super::emit_interfaces_from_compiled`].
//! [`LoadOptions`] controls optional bytecode verification when decoding a linked binary via
//! [`super::load_project_binary_with_options`].
//!
//! Both types are `Copy` and [`Default`]; embedders and the CLI construct them at the public
//! driver boundary and pass them unchanged through dependency prebuild, workspace compile,
//! artifact emission, and binary load.
//!
//! ## Build options
//!
//! | Field | Effect |
//! | --- | --- |
//! | [`BuildOptions::force`] | Bypass incremental staleness checks; rebuild every workspace module and re-link. |
//! | [`BuildOptions::emit_interface_only`] | Write `.pxi` and `manifest.json` only; skip per-module `.phx0` codegen and final link. |
//!
//! When `emit_interface_only` is set, [`super::BuildResult::output_path`] points at
//! `manifest.json` rather than the linked `build/bin/` or `build/lib/` artifact.
//!
//! ## Load options
//!
//! | Field | Effect |
//! | --- | --- |
//! | [`LoadOptions::verify_on_load`] | Run the bytecode verifier after decode (defense in depth at load time). |
//!
//! Decode-only load is the default; enable verification when loading untrusted or
//! externally produced binaries before execution.

/// Controls optional bytecode verification when loading a linked binary.
///
/// Passed to [`super::load_project_binary_with_options`]. The default ([`Default`]) skips
/// verification after decode; set [`LoadOptions::verify_on_load`] when the caller wants
/// the same structural checks applied at compile time to run again at load time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoadOptions {
    /// When `true`, run [`phx_bytecode::verify`] after decode.
    ///
    /// Rejects malformed bytecode that decodes successfully but fails structural checks
    /// (invalid jumps, stack underflow, bad section bounds). Adds compile-time cost at
    /// load; use for defense in depth when executing binaries from disk.
    pub verify_on_load: bool,
}

/// Controls incremental rebuild and interface-only emission for project builds.
///
/// Passed to [`super::build_project`] and [`super::emit_interfaces_from_compiled`].
/// Incremental skips are driven by `build/manifest.json` staleness unless
/// [`BuildOptions::force`] is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuildOptions {
    /// Rebuild all workspace modules regardless of manifest staleness.
    ///
    /// When `true`, incremental freshness checks in the driver treat every module as
    /// stale and re-run resolve, type-check, and artifact emission. Path dependencies
    /// are still built first; this flag applies to the workspace crate and its modules.
    pub force: bool,

    /// Emit `.pxi` and `manifest.json` only; skip per-module `.phx0` codegen and link.
    ///
    /// Used by `phx check --emit-interface-only` and embedders that type-check externally
    /// and only need interface artifacts for downstream crates. When set,
    /// [`super::BuildResult::output_path`] is the manifest path, not the linked binary.
    pub emit_interface_only: bool,
}

impl BuildOptions {
    /// Convenience constructor for forced full builds with interface and object emission.
    ///
    /// Sets [`BuildOptions::force`] to the given value and leaves
    /// [`BuildOptions::emit_interface_only`] as `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use phx_compiler::BuildOptions;
    ///
    /// let opts = BuildOptions::force(true);
    /// assert!(opts.force);
    /// assert!(!opts.emit_interface_only);
    /// ```
    #[must_use]
    pub const fn force(force: bool) -> Self {
        Self {
            force,
            emit_interface_only: false,
        }
    }
}

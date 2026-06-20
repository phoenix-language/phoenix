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
//! | [`BuildOptions::profile`] | Choose dev vs release artifact policy (debug metadata emitted or stripped). |
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

/// Build profile controlling dev vs release PHX0 emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildProfile {
    /// Default local build profile: keep debug metadata such as section 5 spans.
    #[default]
    Dev,
    /// Distribution profile: strip optional debug metadata from emitted PHX0.
    Release,
}

impl BuildProfile {
    /// Returns `true` when this profile should emit debug sections into PHX0.
    #[must_use]
    pub const fn emits_debug_sections(self) -> bool {
        matches!(self, Self::Dev)
    }

    /// Stable manifest token for this profile.
    #[must_use]
    pub const fn as_manifest_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Release => "release",
        }
    }

    /// Parses a profile token loaded from `build/manifest.json`.
    #[must_use]
    pub fn from_manifest_str(value: &str) -> Option<Self> {
        match value {
            "dev" => Some(Self::Dev),
            "release" => Some(Self::Release),
            _ => None,
        }
    }
}

/// Controls incremental rebuild, profile selection, and interface-only emission for project builds.
///
/// Passed to [`super::build_project`] and [`super::emit_interfaces_from_compiled`].
/// Incremental skips are driven by `build/manifest.json` staleness unless
/// [`BuildOptions::force`] is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    /// Artifact profile used for codegen and incremental cache compatibility.
    ///
    /// [`BuildProfile::Dev`] keeps debug sections in emitted `.phx0` objects and linked
    /// outputs. [`BuildProfile::Release`] strips optional debug metadata so section 5 is
    /// absent and `PHX0_HAS_DEBUG` remains clear.
    pub profile: BuildProfile,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            force: false,
            emit_interface_only: false,
            profile: BuildProfile::Dev,
        }
    }
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
            profile: BuildProfile::Dev,
        }
    }

    /// Returns a copy of these options with the given build profile.
    ///
    /// # Examples
    ///
    /// ```
    /// use phx_compiler::{BuildOptions, BuildProfile};
    ///
    /// let opts = BuildOptions::force(true).with_profile(BuildProfile::Release);
    /// assert!(opts.force);
    /// assert_eq!(opts.profile, BuildProfile::Release);
    /// ```
    #[must_use]
    pub const fn with_profile(self, profile: BuildProfile) -> Self {
        Self { profile, ..self }
    }
}

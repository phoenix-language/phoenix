//! Structured exit codes for the `phx` binary.
//!
//! This module is the **final stage** of the CLI pipeline: every path through
//! [`crate::run`] and the command handlers in [`crate::commands`] returns a
//! [`CliExit`] that the `phx` binary converts to a process exit code.
//!
//! ```text
//! parse (args) ──► workflow ──► commands ──► CliExit ──► ExitCode / i32
//!      │                                              │
//!      └── ParseError ──► CliExit::Usage ◄───────────┘
//! ```
//!
//! Variants mirror failure categories so scripts and CI can distinguish usage
//! errors from compile failures, I/O problems, bytecode verification, and VM
//! runtime faults. Internal compiler panics map to [`CliExit::Internal`] via
//! the ICE hook in the binary crate.
//!
//! ## Public types
//!
//! - [`CliExit`] — structured exit status returned by [`crate::run`].

use std::process::ExitCode;

/// Exit status returned by [`crate::run`] and command handlers.
///
/// Convert with [`CliExit::into_exit_code`] for `main` or
/// [`CliExit::as_i32`] when integrating with `std::process::exit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CliExit {
    /// Success.
    Ok = 0,
    /// Compile or diagnostic failure.
    Compile = 1,
    /// Usage or argument parse failure.
    Usage = 2,
    /// I/O failure.
    Io = 3,
    /// Bytecode verify failure.
    Verify = 4,
    /// VM runtime failure.
    Runtime = 5,
    /// Internal compiler or VM panic (should not happen on valid input).
    Internal = 6,
}

impl CliExit {
    /// Converts to [`ExitCode`] for use as the return type of `main`.
    #[must_use]
    pub fn into_exit_code(self) -> ExitCode {
        ExitCode::from(self as u8)
    }

    /// Raw process exit code for [`std::process::exit`].
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self as u8 as i32
    }
}

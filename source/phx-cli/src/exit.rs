//! Structured exit codes for the `phx` binary.

use std::process::ExitCode;

/// Exit status returned by [`crate::run`].
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
    /// Converts to [`ExitCode`].
    #[must_use]
    pub fn into_exit_code(self) -> ExitCode {
        ExitCode::from(self as u8)
    }

    /// Process exit code for [`std::process::exit`].
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self as u8 as i32
    }
}

//! Internal compiler error (ICE) reporting for the `phx` binary boundary.
//!
//! When the compiler or VM panics, the `phx` driver catches the unwind and
//! prints a generic message. Set [`PHX_ICE_DEBUG_ENV`] to `"1"` (or set
//! `RUST_BACKTRACE` to a non-empty value other than `"0"`) to also print the
//! panic message and a backtrace on stderr.

use std::any::Any;
use std::backtrace::Backtrace;

/// Environment variable that enables ICE debug detail when set to `"1"`.
pub const PHX_ICE_DEBUG_ENV: &str = "PHX_ICE_DEBUG";

/// User-facing ICE line printed on every internal panic (exit code 6).
pub const INTERNAL_ERROR_MSG: &str = "internal compiler error: please report with a minimal repro";

/// Returns true when ICE reporting should include panic message and backtrace.
#[must_use]
pub fn ice_debug_enabled() -> bool {
    ice_debug_enabled_from(
        std::env::var(PHX_ICE_DEBUG_ENV).ok().as_deref(),
        std::env::var("RUST_BACKTRACE").ok().as_deref(),
    )
}

/// Testable ICE debug gate from explicit env values (see [`ice_debug_enabled`]).
#[must_use]
pub fn ice_debug_enabled_from(phx_ice_debug: Option<&str>, rust_backtrace: Option<&str>) -> bool {
    phx_ice_debug.is_some_and(|v| v == "1")
        || rust_backtrace.is_some_and(|v| !v.is_empty() && v != "0")
}

/// Extracts a displayable panic message from a caught unwind payload.
#[must_use]
pub fn panic_payload_message(payload: &(dyn Any + Send)) -> Option<String> {
    payload
        .downcast_ref::<&str>()
        .map(|msg| (*msg).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
}

/// Reports an internal compiler error to stderr, optionally with debug detail.
pub fn report_ice(payload: &(dyn Any + Send)) {
    eprintln!("{INTERNAL_ERROR_MSG}");
    if ice_debug_enabled() {
        if let Some(msg) = panic_payload_message(payload) {
            eprintln!("panic message: {msg}");
        }
        let backtrace = Backtrace::force_capture();
        eprintln!("backtrace:\n{backtrace}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ice_debug_off_when_env_unset_or_not_one() {
        assert!(!ice_debug_enabled_from(None, None));
        assert!(!ice_debug_enabled_from(Some("0"), None));
        assert!(!ice_debug_enabled_from(Some(""), None));
        assert!(!ice_debug_enabled_from(Some("true"), None));
    }

    #[test]
    fn ice_debug_on_when_phx_ice_debug_is_one() {
        assert!(ice_debug_enabled_from(Some("1"), None));
    }

    #[test]
    fn ice_debug_follows_rust_backtrace_env() {
        assert!(ice_debug_enabled_from(None, Some("1")));
        assert!(ice_debug_enabled_from(None, Some("full")));
        assert!(!ice_debug_enabled_from(None, Some("0")));
        assert!(!ice_debug_enabled_from(None, Some("")));
    }

    #[test]
    fn panic_payload_message_accepts_str_and_string() {
        let as_str: &str = "integration test forced panic";
        assert_eq!(
            panic_payload_message(&as_str),
            Some("integration test forced panic".to_owned())
        );
        let as_string = "boom".to_owned();
        assert_eq!(panic_payload_message(&as_string), Some("boom".to_owned()));
    }
}

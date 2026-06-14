//! `phx` command-line driver.

#![allow(clippy::print_stderr, clippy::single_match_else)]

use std::any::Any;
use std::backtrace::Backtrace;
use std::panic::{self, AssertUnwindSafe};
use std::process;

use phx_cli::exit::CliExit;

const INTERNAL_ERROR_MSG: &str = "internal compiler error: please report with a minimal repro";
const PHX_ICE_DEBUG_ENV: &str = "PHX_ICE_DEBUG";

fn main() {
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let code = match panic::catch_unwind(AssertUnwindSafe(phx_cli::run)) {
        Ok(exit) => exit,
        Err(payload) => {
            report_ice(&*payload);
            CliExit::Internal
        }
    };

    panic::set_hook(prev_hook);
    process::exit(code.as_i32());
}

fn ice_debug_enabled() -> bool {
    std::env::var(PHX_ICE_DEBUG_ENV).is_ok_and(|v| v == "1")
        || std::env::var("RUST_BACKTRACE").is_ok_and(|v| !v.is_empty() && v != "0")
}

fn panic_payload_message(payload: &(dyn Any + Send)) -> Option<String> {
    payload
        .downcast_ref::<&str>()
        .map(|msg| (*msg).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
}

fn report_ice(payload: &(dyn Any + Send)) {
    eprintln!("{INTERNAL_ERROR_MSG}");
    if ice_debug_enabled() {
        if let Some(msg) = panic_payload_message(payload) {
            eprintln!("panic message: {msg}");
        }
        let backtrace = Backtrace::force_capture();
        eprintln!("backtrace:\n{backtrace}");
    }
}

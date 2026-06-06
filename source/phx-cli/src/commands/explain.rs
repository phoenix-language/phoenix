//! `phx explain` handler.

use phx_diagnostics::{explain_code, normalize_code};

use crate::color::ColorChoice;
use crate::exit::CliExit;
use crate::report::Reporter;

/// Runs `phx explain <code>`.
pub fn run_explain(code: String, color: ColorChoice) -> CliExit {
    let style = crate::color::diagnostic_style(color);
    let reporter = Reporter::new(&style);

    let Some(normalized) = normalize_code(&code) else {
        reporter.usage_error(&format!(
            "invalid diagnostic code '{code}' (expected E####)"
        ));
        return CliExit::Usage;
    };

    match explain_code(&normalized) {
        Some(text) => {
            eprintln!("{normalized}: {text}");
            CliExit::Ok
        }
        None => {
            reporter.usage_error(&format!("no explanation available for {normalized}"));
            CliExit::Usage
        }
    }
}

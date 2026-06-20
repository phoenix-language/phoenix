//! PHX-040: malformed `.pxi` input must return `Err` without panic.

mod support;

use phx_compiler::PxiFile;
use support::test_err;

/// Expected `.pxi` parse failure (matches [`phx_compiler::pxi::PxiError`] display).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpectedPxiError<'a> {
    UnsupportedVersion(u32),
    Parse(&'a str),
}

impl ExpectedPxiError<'_> {
    fn assert_matches(&self, err: &impl std::fmt::Display) {
        let expected = match self {
            Self::UnsupportedVersion(found) => format!("unsupported .pxi format version {found}"),
            Self::Parse(message) => format!("invalid .pxi: {message}"),
        };
        assert_eq!(err.to_string(), expected);
    }
}

fn minimal_pxi_json(exports: &str, dependencies: &str) -> String {
    format!(
        r#"{{
  "format_version": 1,
  "logical_module": "m",
  "source_hash": "h",
  "origin": null,
  "exports": {exports},
  "dependencies": {dependencies}
}}"#
    )
}

#[test]
fn malformed_pxi_inputs_return_err_without_panic() {
    let cases: &[(&str, ExpectedPxiError<'_>)] = &[
        (
            r#"{"format_version": 99, "logical_module": "m", "source_hash": "h", "exports": [], "dependencies": []}"#,
            ExpectedPxiError::UnsupportedVersion(99),
        ),
        (
            r#"{"logical_module": "m", "source_hash": "h", "exports": [], "dependencies": []}"#,
            ExpectedPxiError::Parse("missing format_version"),
        ),
        (
            r#"{"format_version": 1, "source_hash": "h", "exports": [], "dependencies": []}"#,
            ExpectedPxiError::Parse("missing logical_module"),
        ),
        (
            r#"{"format_version": 1, "logical_module": "m", "exports": [], "dependencies": []}"#,
            ExpectedPxiError::Parse("missing source_hash"),
        ),
        (
            r#"{
  "format_version": 1,
  "logical_module": "m",
  "source_hash": "h",
  "origin": null,
  "dependencies": []
}"#,
            ExpectedPxiError::Parse("missing exports"),
        ),
        (
            r#"{
  "format_version": 1,
  "logical_module": "m",
  "source_hash": "h",
  "origin": null,
  "exports": [
    {"export_id": "m::add::fn", "name": "add", "kind": "fn", "signature": "() => ()"},
"#,
            ExpectedPxiError::Parse("malformed exports: truncated exports array"),
        ),
        (
            &minimal_pxi_json("[ 123 ]", "[]"),
            ExpectedPxiError::Parse("malformed exports: expected object or ']'"),
        ),
        (
            &minimal_pxi_json(
                r#"[{"export_id": "m::add::fn", "kind": "fn", "signature": "() => ()"}]"#,
                "[]",
            ),
            ExpectedPxiError::Parse("malformed export: missing 'name'"),
        ),
        (
            &minimal_pxi_json("[]", r#"[{"logical_module": "core"}]"#),
            ExpectedPxiError::Parse("malformed dependency: missing 'pxi_hash'"),
        ),
    ];

    for (input, expected) in cases {
        let err = test_err(PxiFile::parse(input), "parse pxi");
        expected.assert_matches(&err);
    }
}

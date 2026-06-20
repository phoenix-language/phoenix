//! Drift tests: every stable diagnostic code has a `phx explain` entry.

#[cfg(test)]
mod tests {
    use crate::explain_code;

    fn assert_explain_codes(codes: &[&str]) {
        for code in codes {
            assert!(
                explain_code(code).is_some(),
                "missing phx explain entry for {code}"
            );
        }
    }

    #[test]
    fn lex_codes_have_explain_entry() {
        assert_explain_codes(&[
            "E0001", "E0002", "E0003", "E0004", "E0005", "E0006", "E0007", "E0008", "E0009",
        ]);
    }

    #[test]
    fn parse_codes_have_explain_entry() {
        assert_explain_codes(&["E3001", "E3002", "E3003", "E3004", "E3005"]);
    }

    #[test]
    fn resolve_codes_have_explain_entry() {
        assert_explain_codes(&[
            "E1001", "E1002", "E1003", "E1004", "E1005", "E1006", "E1007", "E1008", "E1009",
            "E1010", "E1011", "E1012", "E1013", "E1014", "E1015", "E1016", "E1017", "E1018",
            "E1019", "E1020", "E1021", "E1022", "E1023", "E1024",
        ]);
    }

    #[test]
    fn lower_and_ir_codes_have_explain_entry() {
        assert_explain_codes(&["E4001", "E4002"]);
    }

    #[test]
    fn lint_codes_have_explain_entry() {
        assert_explain_codes(&["W3001", "W3002"]);
    }

    #[test]
    fn typeck_copyable_drop_has_explain_entry() {
        assert_explain_codes(&["E2033"]);
    }
}

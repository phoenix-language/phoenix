//! Language item registry tests.

mod support;

use phx_compiler::build_lang_item_registry;
use phx_compiler::{CompileError, compile_source};
use phx_diagnostics::TypeCheckBag;
use support::{test_err, test_ok};

#[test]
fn user_lang_item_rejected() {
    let source = concat!(
        "#[lang_item(name = \"alloc_bytes\", kind = \"intrinsic\")]\n",
        "alloc_bytes :: (size: u32) => *mut u8 { size as *mut u8 };\n\n",
        "main :: () => {};\n",
    );
    let err = test_err(compile_source(source, None), "compile");
    let CompileError::TypeCheck { bag, .. } = err else {
        panic!("expected type-check failure");
    };
    assert!(bag.errors().iter().any(|e| matches!(
        e.error,
        phx_diagnostics::TypeCheckError::LangItemReserved { .. }
    )));
}

#[test]
fn build_registry_empty_without_std() {
    let source = "main :: () => {};\n";
    let unit = test_ok(compile_source(source, None), "compile");
    let mut bag = TypeCheckBag::new();
    let registry = build_lang_item_registry(&unit.typed.resolved, &mut bag);
    assert!(registry.option_enum.is_none());
    assert!(!bag.has_errors());
}

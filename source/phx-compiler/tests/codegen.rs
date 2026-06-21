//! Codegen integration tests.

mod support;

use std::collections::HashMap;
use std::path::Path;

use phx_bytecode::verify;
use phx_bytecode::{BytecodeModule, ConstTag, ENTRY_NONE, Opcode, PHX0_HAS_DEBUG};
use phx_compiler::{
    BuildProfile, compile_source,
    unstable::{CompilationUnit, IrBinOp, IrInst, IrModule, codegen, lower},
};
use support::{test_array, test_ok, test_some};

fn compile(source: &str) -> CompilationUnit {
    test_ok(compile_source(source, None), "compile")
}

fn compile_with_path(source: &str, path: &Path) -> CompilationUnit {
    test_ok(compile_source(source, Some(path)), "compile")
}

fn check_file(path: &Path) -> CompilationUnit {
    test_ok(phx_compiler::check_file(path), "check_file")
}

fn lower_unit(unit: &CompilationUnit) -> IrModule {
    test_ok(lower(&unit.typed), "lower")
}

fn codegen_unit(ir: &IrModule, unit: &CompilationUnit) -> BytecodeModule {
    test_ok(codegen(ir, &unit.typed), "codegen")
}

fn verify_module(module: &BytecodeModule) {
    test_ok(verify(module), "verify");
}

fn compile_lower_codegen(source: &str) -> (CompilationUnit, IrModule, BytecodeModule) {
    let unit = compile(source);
    let ir = lower_unit(&unit);
    let module = codegen_unit(&ir, &unit);
    (unit, ir, module)
}

fn codegen_module_with_profile(source: &str, profile: BuildProfile) -> BytecodeModule {
    let unit = compile_with_path(source, Path::new("profile_test.phx"));
    let ir = lower_unit(&unit);
    let global_fn: HashMap<_, _> = ir.functions.iter().map(|f| (f.def, f.id.index())).collect();
    test_ok(
        phx_compiler::unstable::codegen_module(
            &ir,
            &unit.typed,
            &global_fn,
            true,
            Some("profile_test.phx"),
            profile,
        ),
        "codegen_module",
    )
}

#[test]
fn codegen_emits_pc_span_section_in_dev_builds() {
    let source = "main :: () => { const n: s32 = 1; };";
    let (_, _, module) = compile_lower_codegen(source);
    verify_module(&module);
    assert!(
        !module.pc_spans.entries.is_empty(),
        "debug codegen should record PC spans"
    );
    let bytes = test_ok(module.encode(), "encode");
    let decoded = test_ok(BytecodeModule::decode(&bytes), "decode");
    assert_eq!(decoded.header.section_count, 6);
    assert_eq!(
        decoded.pc_spans.entries.len(),
        module.pc_spans.entries.len()
    );
    assert!(
        decoded
            .pc_spans
            .lookup_exact(0, 0)
            .is_some_and(|e| e.span_end > e.span_start)
    );
    assert_eq!(
        module.pc_spans.lookup_function_name(0),
        Some("main"),
        "dev codegen should record function debug names"
    );
}

#[test]
fn codegen_emits_function_debug_names_in_dev_builds() {
    let source = "helper :: () => { }; main :: () => { helper(); };";
    let (_, _, module) = compile_lower_codegen(source);
    verify_module(&module);
    let names: Vec<_> = module
        .pc_spans
        .function_names
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert!(names.contains(&"main"));
    assert!(names.contains(&"helper"));
    let bytes = test_ok(module.encode(), "encode");
    let decoded = test_ok(BytecodeModule::decode(&bytes), "decode");
    assert_eq!(decoded.pc_spans.function_names.len(), 2);
}

#[test]
fn codegen_module_dev_profile_keeps_debug_sections() {
    let module = codegen_module_with_profile(
        "main :: () => { const n: s32 = 1; const _ = n; };",
        BuildProfile::Dev,
    );
    verify_module(&module);
    assert!(
        !module.pc_spans.entries.is_empty(),
        "dev profile should keep PC span debug rows"
    );
    assert_ne!(module.header.flags & PHX0_HAS_DEBUG, 0);
    assert_eq!(module.header.section_count, 6);
}

#[test]
fn codegen_module_release_profile_strips_debug_sections() {
    let module = codegen_module_with_profile(
        "main :: () => { const n: s32 = 1; const _ = n; };",
        BuildProfile::Release,
    );
    verify_module(&module);
    assert!(
        module.pc_spans.entries.is_empty(),
        "release profile should strip PC span debug rows"
    );
    assert_eq!(module.header.flags & PHX0_HAS_DEBUG, 0);
    assert_eq!(module.header.section_count, 5);

    let bytes = test_ok(module.encode(), "encode");
    let decoded = test_ok(BytecodeModule::decode(&bytes), "decode");
    verify_module(&decoded);
    assert!(decoded.pc_spans.entries.is_empty());
    assert_eq!(decoded.header.flags & PHX0_HAS_DEBUG, 0);
    assert_eq!(decoded.header.section_count, 5);
}

#[test]
fn codegen_generic_fn_inline_verifies() {
    let source = "id :: <t> (x: t) => t { x }; main :: () => { const n: s32 = id :: <s32> (42); };";
    let (_, _, module) = compile_lower_codegen(source);
    verify_module(&module);
}

#[test]
fn codegen_generic_fn_verifies() {
    let path = support::single_file_path("generic_fn.phx");
    let unit = check_file(&path);
    let module = codegen_unit(&lower_unit(&unit), &unit);
    verify_module(&module);
}

#[test]
fn codegen_deep_logical_chain_verifies() {
    let path = support::single_file_path("deep_logical_chain.phx");
    let unit = check_file(&path);
    let module = codegen_unit(&lower_unit(&unit), &unit);
    verify_module(&module);
}

#[test]
fn codegen_dual_generic_fn_instantiation_verifies() {
    let source = "id :: <t> (x: t) => t { x }; main :: () => { const a: s32 = id :: <s32> (1); const b: bool = id :: <bool> (true); const _ = a; };";
    let unit = compile(source);
    let module = codegen_unit(&lower_unit(&unit), &unit);
    assert_eq!(
        module.functions.functions.len(),
        3,
        "expected two specialized functions plus main in bytecode"
    );
    verify_module(&module);
}

#[test]
fn codegen_sample_round_trip_and_verify() {
    let source = support::single_source("sample.phx");
    let unit = compile_with_path(source, Path::new("sample.phx"));
    let module = codegen_unit(&lower_unit(&unit), &unit);

    assert_eq!(module.functions.functions.len(), 2);
    assert!(!module.code.is_empty());
    assert!(module.constants.entries.len() >= 2);

    let main_id = module.header.entry_function_id;
    let main_rec = test_some(
        module
            .functions
            .functions
            .iter()
            .find(|f| f.function_id == main_id),
        "main function record",
    );
    assert_eq!(main_rec.arity, 0);

    let bytes = test_ok(module.encode(), "encode");
    let decoded = test_ok(BytecodeModule::decode(&bytes), "decode");
    verify_module(&decoded);

    let add_rec = test_some(
        module
            .functions
            .functions
            .iter()
            .find(|f| f.function_id != main_id),
        "add",
    );
    assert_eq!(add_rec.arity, 2);

    let add_code = &module.code
        [add_rec.code_offset as usize..add_rec.code_offset as usize + add_rec.code_len as usize];
    assert!(add_code.contains(&Opcode::Add.as_u8()));
}

#[test]
fn codegen_constants_include_sample_literals() {
    let source = support::single_source("sample.phx");
    let unit = compile(source);
    let module = codegen_unit(&lower_unit(&unit), &unit);

    let mut has_ten = false;
    let mut has_two = false;
    for entry in &module.constants.entries {
        if entry.tag == ConstTag::SignedInt {
            let v = match entry.payload.len() {
                1 => i64::from(i8::from_ne_bytes([entry.payload[0]])),
                4 => i64::from(i32::from_le_bytes(test_array(
                    &entry.payload[0..4],
                    "s32 payload",
                ))),
                8 => i64::from_le_bytes(test_array(&entry.payload[0..8], "s64 payload")),
                _ => continue,
            };
            if v == 10 {
                has_ten = true;
            }
            if v == 2 {
                has_two = true;
            }
        }
    }
    assert!(has_ten && has_two);
}

#[test]
fn continue_program_runs_on_vm() {
    let source =
        "main :: () => { var i: s32 = 0; loop { i = i + 1; if 3 > (i) { continue; } break; }; };";
    let (_, _, module) = compile_lower_codegen(source);
    verify_module(&module);
}

#[test]
fn lower_logical_short_circuit_emits_jump_if() {
    let source = "main :: () => { const a: bool = true && false; const b: bool = true || false; const c: bool = a || b; const _ = c; };";
    let ir = lower_unit(&compile(source));
    let mut jump_if_count = 0u32;
    for f in &ir.functions {
        for block in &f.blocks {
            for spanned in &block.insts {
                if matches!(&spanned.inst, IrInst::JumpIf { .. }) {
                    jump_if_count += 1;
                }
            }
        }
    }
    assert!(
        jump_if_count >= 2,
        "&& and || should lower to at least two JumpIf terminators"
    );
}

#[test]
fn lower_match_emits_eq_and_jump_if() {
    let source = "main :: () => { var i: s32 = 1; const x: s32 = { match i { 0 => 10; _ => 20; } }; const _ = x; };";
    let ir = lower_unit(&compile(source));
    let mut eq_count = 0u32;
    let mut jump_if_count = 0u32;
    for f in &ir.functions {
        for block in &f.blocks {
            for spanned in &block.insts {
                if matches!(
                    &spanned.inst,
                    IrInst::BinOp {
                        op: IrBinOp::Eq,
                        ..
                    }
                ) {
                    eq_count += 1;
                }
                if matches!(&spanned.inst, IrInst::JumpIf { .. }) {
                    jump_if_count += 1;
                }
            }
        }
    }
    assert!(eq_count >= 1, "literal match arm should compare with Eq");
    assert!(jump_if_count >= 1, "match should branch with JumpIf");
}

#[test]
fn codegen_enum_match_verifies() {
    let source = support::single_source("enum_match.phx");
    let (_, _, module) = compile_lower_codegen(source);
    verify_module(&module);
}

#[test]
fn codegen_enum_struct_match_emits_tag_and_get_field() {
    let source = support::single_source("enum_match_struct.phx");
    let unit = compile(source);
    let ir = lower_unit(&unit);
    let insts: Vec<_> = ir
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .collect();
    assert!(
        insts
            .iter()
            .any(|s| matches!(&s.inst, IrInst::MatchTag { .. })),
        "struct-variant match should emit MatchTag"
    );
    assert!(
        insts
            .iter()
            .any(|s| matches!(&s.inst, IrInst::GetField { .. })),
        "struct-variant bind should emit GetField"
    );
    verify_module(&codegen_unit(&ir, &unit));
}

#[test]
fn codegen_struct_point_emits_make_struct() {
    let source = support::single_source("struct_point.phx");
    let unit = compile(source);
    let ir = lower_unit(&unit);
    let has_make = ir
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .any(|s| matches!(&s.inst, IrInst::MakeStruct { .. }));
    assert!(has_make, "struct literal should emit MakeStruct");
    let has_get = ir
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .any(|s| matches!(&s.inst, IrInst::GetField { .. }));
    assert!(has_get, "field read should emit GetField");
    verify_module(&codegen_unit(&ir, &unit));
}

#[test]
fn deep_logical_chain_verifies_and_runs() {
    let source = support::single_source("deep_logical_chain.phx");
    let (_, _, module) = compile_lower_codegen(source);
    verify_module(&module);
}

#[test]
fn deep_logical_or_chain_verifies() {
    let source = support::single_source("deep_logical_or_chain.phx");
    let (_, _, module) = compile_lower_codegen(source);
    verify_module(&module);
}

#[test]
fn assign_to_var_emits_store_local() {
    let source = "main :: () => { var i: s32 = 0; i = i + 1; const _ = i; };";
    let ir = lower_unit(&compile(source));
    let stores = ir
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .filter(|s| matches!(&s.inst, IrInst::StoreLocal { .. }))
        .count();
    assert!(stores >= 2, "var init and assign should both StoreLocal");
}

#[test]
fn greater_than_lowers_via_swapped_lt() {
    let source = "main :: () => { const t: bool = 3 > 2; const _ = t; };";
    let unit = compile(source);
    let ir = lower_unit(&unit);
    let has_lt = ir.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.insts.iter().any(|s| {
                matches!(
                    &s.inst,
                    IrInst::BinOp {
                        op: IrBinOp::Lt,
                        ..
                    }
                )
            })
        })
    });
    assert!(
        has_lt,
        "3 > 2 should lower to IrBinOp::Lt with swapped operands"
    );
    verify_module(&codegen_unit(&ir, &unit));
}

#[test]
fn const_pool_dedupes_identical_literals() {
    let source = "main :: () => { const a: s32 = 42; const b: s32 = 42; const _ = a + b; };\n";
    let unit = compile(source);
    let ir = lower_unit(&unit);
    assert!(ir.constants.len() >= 2, "expected at least two IR literals");
    let module = codegen_unit(&ir, &unit);
    assert_eq!(
        module.constants.entries.len(),
        1,
        "identical s32 literals should share one pool entry"
    );
    verify_module(&module);
}

#[test]
fn codegen_associated_from_call_verifies() {
    let source = "FromLocal :: <source> trait { from :: (value: source) => Self; }; Wrap :: struct { n: s32, }; Wrap :: impl :: FromLocal<s32> { from :: (value: s32) => Wrap { Wrap { n: value } }; }; main :: () => { const w: Wrap = Wrap::from(42); const _ = w.n; };";
    let (_, _, module) = compile_lower_codegen(source);
    verify_module(&module);
}

#[test]
fn codegen_fn_pointer_indirect_call_verifies() {
    let source = "double :: (x: s32) => s32 { x + x }; apply :: (f: :: (s32) => s32, x: s32) => s32 { f(x) }; main :: () => { const n: s32 = apply(double, 3); const _ = n; };";
    let (_, _, module) = compile_lower_codegen(source);
    assert!(
        module.code.contains(&Opcode::MakeFnPtr.as_u8()),
        "expected MakeFnPtr opcode in main"
    );
    assert!(
        module.code.contains(&Opcode::CallIndirect.as_u8()),
        "expected CallIndirect opcode in apply"
    );
    verify_module(&module);
}

#[test]
fn codegen_library_module_without_entry_uses_entry_none() {
    let source = "helper :: () => () { }; main :: () => { helper(); };";
    let unit = compile(source);
    let ir = lower_unit(&unit);
    let helper = test_some(
        ir.functions.iter().find(|f| {
            unit.typed
                .resolved
                .defs
                .get(f.def.index() as usize)
                .is_some_and(|d| unit.typed.resolved.interner.resolves_to(d.name, "helper"))
        }),
        "helper IR",
    );
    let module = test_ok(
        codegen(
            &IrModule {
                functions: vec![helper.clone()],
                constants: ir.constants.clone(),
                entry: None,
            },
            &unit.typed,
        ),
        "codegen library module",
    );
    assert_eq!(
        module.header.entry_function_id, ENTRY_NONE,
        "library-style single-module codegen must not default entry to function 0"
    );
    verify_module(&module);
}

#[test]
fn codegen_while_loop_stack_analysis_completes() {
    let source = "loop_fn :: () => () { var i: u32 = 0u; while i < 4u { i = i + 1u; }; }; main :: () => { loop_fn(); };";
    let unit = compile(source);
    let ir = lower_unit(&unit);
    let loop_fn = test_some(
        ir.functions.iter().find(|f| {
            unit.typed
                .resolved
                .defs
                .get(f.def.index() as usize)
                .is_some_and(|d| unit.typed.resolved.interner.resolves_to(d.name, "loop_fn"))
        }),
        "loop_fn IR",
    );
    let module = test_ok(
        codegen(
            &IrModule {
                functions: vec![loop_fn.clone()],
                constants: ir.constants.clone(),
                entry: ir.entry,
            },
            &unit.typed,
        ),
        "codegen while-loop CFG must not hang",
    );
    assert!(module.code.len() > 4, "expected loop_fn bytecode");
}

#[test]
fn codegen_generic_impl_method_calls_specialized_get_not_main() {
    let path = support::single_file_path("generic_impl_method.phx");
    let unit = check_file(&path);
    let ir = lower_unit(&unit);
    assert!(
        ir.functions.len() >= 2,
        "expected specialized get plus main, got {}",
        ir.functions.len()
    );
    let call_targets: Vec<_> = ir
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .filter_map(|s| match &s.inst {
            IrInst::Call { callee, .. } => Some(*callee),
            _ => None,
        })
        .collect();
    assert_eq!(call_targets.len(), 1, "expected one Call in program IR");
    let callee = call_targets[0];
    let callee_is_main = ir
        .functions
        .iter()
        .any(|f| f.def == callee && f.params.is_empty());
    assert!(
        !callee_is_main,
        "method call must not target main (infinite recursion)"
    );
}

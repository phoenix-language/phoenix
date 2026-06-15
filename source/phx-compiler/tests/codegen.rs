//! Codegen integration tests.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use phx_bytecode::verify;
use phx_bytecode::{BytecodeModule, ConstTag, ENTRY_NONE, Opcode};
use phx_compiler::{
    compile_source,
    unstable::{IrBinOp, IrInst, IrModule, codegen, lower},
};

#[test]
fn codegen_generic_fn_inline_verifies() {
    let source = "id :: <t> (x: t) => t { x }; main :: () => { const n: s32 = id :: <s32> (42); };";
    let unit = compile_source(source, None).expect("compile_source");
    let ir = lower(&unit.typed).expect("lower");
    let module = codegen(&ir, &unit.typed).expect("codegen");
    verify(&module).expect("verify inline generic_fn");
}

#[test]
fn codegen_generic_fn_verifies() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cli/fixtures/generic_fn.phx");
    let unit = phx_compiler::check_file(&path).expect("check_file");
    let ir = lower(&unit.typed).expect("lower");
    let module = codegen(&ir, &unit.typed).expect("codegen");
    verify(&module).expect("verify generic_fn");
}

#[test]
fn codegen_deep_logical_chain_verifies() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/deep_logical_chain.phx");
    let unit = phx_compiler::check_file(&path).expect("check_file");
    let ir = lower(&unit.typed).expect("lower");
    let module = codegen(&ir, &unit.typed).expect("codegen");
    verify(&module).expect("verify deep logical chain");
}

#[test]
fn codegen_dual_generic_fn_instantiation_verifies() {
    let source = "id :: <t> (x: t) => t { x }; main :: () => { const a: s32 = id :: <s32> (1); const b: bool = id :: <bool> (true); const _ = a; };";
    let unit = compile_source(source, None).expect("compile dual generic fn");
    let module = codegen(&lower(&unit.typed).expect("lower"), &unit.typed).expect("codegen");
    assert_eq!(
        module.functions.functions.len(),
        3,
        "expected two specialized functions plus main in bytecode"
    );
    verify(&module).expect("verify dual generic fn instantiation");
}

#[test]
fn codegen_sample_round_trip_and_verify() {
    let source = include_str!("../../../tests/cli/fixtures/sample.phx");
    let unit = compile_source(source, Some(Path::new("sample.phx")))
        .unwrap_or_else(|e| panic!("compile: {e}"));
    let ir = lower(&unit.typed).expect("lower");
    let module = codegen(&ir, &unit.typed).expect("codegen");

    assert_eq!(module.functions.functions.len(), 2);
    assert!(!module.code.is_empty());
    assert!(module.constants.entries.len() >= 2);

    let main_id = module.header.entry_function_id;
    let main_rec = module
        .functions
        .functions
        .iter()
        .find(|f| f.function_id == main_id)
        .expect("main function record");
    assert_eq!(main_rec.arity, 0);

    let bytes = module.encode().expect("encode");
    let decoded = BytecodeModule::decode(&bytes).expect("decode");
    verify(&decoded).expect("verify");

    let add_rec = module
        .functions
        .functions
        .iter()
        .find(|f| f.function_id != main_id)
        .expect("add");
    assert_eq!(add_rec.arity, 2);

    let add_code = &module.code
        [add_rec.code_offset as usize..add_rec.code_offset as usize + add_rec.code_len as usize];
    assert!(add_code.contains(&Opcode::Add.as_u8()));
}

#[test]
fn codegen_constants_include_sample_literals() {
    let source = include_str!("../../../tests/cli/fixtures/sample.phx");
    let unit = compile_source(source, None).unwrap();
    let module = codegen(&lower(&unit.typed).expect("lower"), &unit.typed).expect("codegen");

    let mut has_ten = false;
    let mut has_two = false;
    for entry in &module.constants.entries {
        if entry.tag == ConstTag::SignedInt {
            let v = match entry.payload.len() {
                1 => i64::from(i8::from_ne_bytes([entry.payload[0]])),
                4 => i64::from(i32::from_le_bytes(entry.payload[0..4].try_into().unwrap())),
                8 => i64::from_le_bytes(entry.payload[0..8].try_into().unwrap()),
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
    let unit = compile_source(source, None).unwrap();
    let module = codegen(&lower(&unit.typed).expect("lower"), &unit.typed).expect("codegen");
    verify(&module).expect("verify continue program");
}

#[test]
fn lower_logical_short_circuit_emits_jump_if() {
    let source = "main :: () => { const a: bool = true && false; const b: bool = true || false; const c: bool = a || b; const _ = c; };";
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed).expect("lower");
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
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed).expect("lower");
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
    let source = include_str!("../../../tests/cli/fixtures/enum_match.phx");
    let unit = compile_source(source, None).unwrap();
    let module = codegen(&lower(&unit.typed).expect("lower"), &unit.typed).expect("codegen");
    verify(&module).expect("enum_match bytecode should verify");
}

#[test]
fn codegen_enum_struct_match_emits_tag_and_get_field() {
    let source = include_str!("../../../tests/cli/fixtures/enum_match_struct.phx");
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed).expect("lower");
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
    let module = codegen(&ir, &unit.typed).expect("codegen");
    verify(&module).expect("enum_match_struct bytecode should verify");
}

#[test]
fn codegen_struct_point_emits_make_struct() {
    let source = include_str!("../../../tests/cli/fixtures/struct_point.phx");
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed).expect("lower");
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
    let module = codegen(&ir, &unit.typed).expect("codegen");
    verify(&module).expect("struct_point bytecode should verify");
}

#[test]
fn deep_logical_chain_verifies_and_runs() {
    let source = include_str!("../../../tests/cli/fixtures/deep_logical_chain.phx");
    let unit = compile_source(source, None).unwrap();
    let module = codegen(&lower(&unit.typed).expect("lower"), &unit.typed).expect("codegen");
    verify(&module).expect("deep && chain should verify");
}

#[test]
fn deep_logical_or_chain_verifies() {
    let source = include_str!("../../../tests/cli/fixtures/deep_logical_or_chain.phx");
    let unit = compile_source(source, None).unwrap();
    let module = codegen(&lower(&unit.typed).expect("lower"), &unit.typed).expect("codegen");
    verify(&module).expect("deep || chain should verify");
}

#[test]
fn assign_to_var_emits_store_local() {
    let source = "main :: () => { var i: s32 = 0; i = i + 1; const _ = i; };";
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed).expect("lower");
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
    let unit = compile_source(source, None).unwrap();
    let ir = lower(&unit.typed).expect("lower");
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
    let module = codegen(&ir, &unit.typed).expect("codegen");
    verify(&module).expect("gt program verifies");
}

#[test]
fn const_pool_dedupes_identical_literals() {
    let source = "main :: () => { const a: s32 = 42; const b: s32 = 42; const _ = a + b; };\n";
    let unit = compile_source(source, None).unwrap_or_else(|e| panic!("compile: {e}"));
    let ir = lower(&unit.typed).expect("lower");
    assert!(ir.constants.len() >= 2, "expected at least two IR literals");
    let module = codegen(&ir, &unit.typed).expect("codegen");
    assert_eq!(
        module.constants.entries.len(),
        1,
        "identical s32 literals should share one pool entry"
    );
    verify(&module).expect("deduped module verifies");
}

#[test]
fn codegen_associated_from_call_verifies() {
    let source = "FromLocal :: <source> trait { from :: (value: source) => Self; }; Wrap :: struct { n: s32, }; Wrap :: impl :: FromLocal<s32> { from :: (value: s32) => Wrap { Wrap { n: value } }; }; main :: () => { const w: Wrap = Wrap::from(42); const _ = w.n; };";
    let unit = compile_source(source, None).expect("compile associated from");
    let module = codegen(&lower(&unit.typed).expect("lower"), &unit.typed).expect("codegen");
    verify(&module).expect("verify associated from call");
}

#[test]
fn codegen_fn_pointer_indirect_call_verifies() {
    let source = "double :: (x: s32) => s32 { x + x }; apply :: (f: :: (s32) => s32, x: s32) => s32 { f(x) }; main :: () => { const n: s32 = apply(double, 3); const _ = n; };";
    let unit = compile_source(source, None).expect("compile fn pointer program");
    let module = codegen(&lower(&unit.typed).expect("lower"), &unit.typed).expect("codegen");
    assert!(
        module.code.contains(&Opcode::MakeFnPtr.as_u8()),
        "expected MakeFnPtr opcode in main"
    );
    assert!(
        module.code.contains(&Opcode::CallIndirect.as_u8()),
        "expected CallIndirect opcode in apply"
    );
    verify(&module).expect("verify fn pointer indirect call module");
}

#[test]
fn codegen_library_module_without_entry_uses_entry_none() {
    let source = "helper :: () => () { }; main :: () => { helper(); };";
    let unit = compile_source(source, None).expect("compile");
    let ir = lower(&unit.typed).expect("lower");
    let helper = ir
        .functions
        .iter()
        .find(|f| {
            unit.typed
                .resolved
                .defs
                .get(f.def.index() as usize)
                .is_some_and(|d| unit.typed.resolved.interner.resolves_to(d.name, "helper"))
        })
        .expect("helper IR");
    let module = codegen(
        &IrModule {
            functions: vec![helper.clone()],
            constants: ir.constants.clone(),
            entry: None,
        },
        &unit.typed,
    )
    .expect("codegen library module");
    assert_eq!(
        module.header.entry_function_id, ENTRY_NONE,
        "library-style single-module codegen must not default entry to function 0"
    );
    verify(&module).expect("library module without entry verifies");
}

#[test]
fn codegen_while_loop_stack_analysis_completes() {
    let source = "loop_fn :: () => () { var i: u32 = 0u; while i < 4u { i = i + 1u; }; }; main :: () => { loop_fn(); };";
    let unit = compile_source(source, None).expect("compile");
    let ir = lower(&unit.typed).expect("lower");
    let loop_fn = ir
        .functions
        .iter()
        .find(|f| {
            unit.typed
                .resolved
                .defs
                .get(f.def.index() as usize)
                .is_some_and(|d| unit.typed.resolved.interner.resolves_to(d.name, "loop_fn"))
        })
        .expect("loop_fn IR");
    let module = codegen(
        &IrModule {
            functions: vec![loop_fn.clone()],
            constants: ir.constants.clone(),
            entry: ir.entry,
        },
        &unit.typed,
    )
    .expect("codegen while-loop CFG must not hang");
    assert!(module.code.len() > 4, "expected loop_fn bytecode");
}

#[test]
fn codegen_generic_impl_method_calls_specialized_get_not_main() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/cli/fixtures/generic_impl_method.phx");
    let unit = phx_compiler::check_file(&path).expect("check_file");
    let ir = lower(&unit.typed).expect("lower");
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

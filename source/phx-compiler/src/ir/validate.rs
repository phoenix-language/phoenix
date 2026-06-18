//! IR validation between lowering and codegen.

use phx_diagnostics::{IrBag, IrError, IrResult};

use super::stack_effect::{analyze_ir_stack_cfg, is_ir_terminator};
use super::{IrFunction, IrInst, IrModule, SpannedInst};
use crate::typeck::TypedProgram;

/// Placeholder base for loop exit targets before [`crate::lower::LowerCtx::patch_loop_exit_targets`].
const LOOP_EXIT_TARGET_BASE: u32 = 0xF000_0000;

/// Returns whether IR validation should run before codegen.
///
/// Enabled in debug/test builds, or when `PHX_VALIDATE_IR=1` is set (including release builds).
#[must_use]
pub fn validation_enabled() -> bool {
    cfg!(any(debug_assertions, test))
        || std::env::var("PHX_VALIDATE_IR")
            .ok()
            .is_some_and(|v| v == "1")
}

/// Validates every function in `ir`.
///
/// Runs structural CFG checks and stack-depth simulation at merge blocks.
/// Call sites gate invocation with [`validation_enabled`].
///
/// # Errors
///
/// Returns [`IrBag`] when any function fails validation.
pub fn validate_ir(ir: &IrModule, typed: &TypedProgram) -> IrResult<()> {
    let mut bag = IrBag::new();
    for func in &ir.functions {
        if let Err(err) = validate_function(func, typed) {
            let module = module_for_def(typed, func.def);
            bag.push(module, err);
        }
    }
    if bag.has_errors() { Err(bag) } else { Ok(()) }
}

/// Validates one lowered function's CFG and stack discipline.
///
/// # Errors
///
/// Returns the first [`IrError`] encountered.
pub fn validate_function(func: &IrFunction, typed: &TypedProgram) -> Result<(), IrError> {
    validate_structure(func)?;
    analyze_ir_stack_cfg(func, typed)
}

fn validate_structure(func: &IrFunction) -> Result<(), IrError> {
    let def_index = func.def.index();
    let block_count = u32::try_from(func.blocks.len()).unwrap_or(u32::MAX);

    if func.blocks.is_empty() {
        return Err(IrError::EmptyFunction { def_index });
    }

    for (block_id, block) in func.blocks.iter().enumerate() {
        let block_u32 = u32::try_from(block_id).unwrap_or(u32::MAX);
        let mut seen_terminator = false;

        for (inst_index, spanned) in block.insts.iter().enumerate() {
            let inst_u32 = u32::try_from(inst_index).unwrap_or(u32::MAX);
            if seen_terminator {
                return Err(IrError::InstructionAfterTerminator {
                    def_index,
                    block: block_u32,
                    inst_index: inst_u32,
                    span: spanned.span,
                });
            }

            validate_jump_targets(func, def_index, block_u32, spanned, block_count)?;

            if is_ir_terminator(&spanned.inst) {
                seen_terminator = true;
            }
        }

        let has_successor = block_id + 1 < func.blocks.len();
        if !seen_terminator && !has_successor {
            return Err(IrError::MissingTerminator {
                def_index,
                block: block_u32,
            });
        }
    }

    Ok(())
}

fn validate_jump_targets(
    func: &IrFunction,
    def_index: u32,
    block: u32,
    spanned: &SpannedInst,
    block_count: u32,
) -> Result<(), IrError> {
    let inst = &spanned.inst;
    let span = spanned.span;
    let check = |target: u32| -> Result<(), IrError> {
        if target >= LOOP_EXIT_TARGET_BASE {
            return Err(IrError::UnpatchedLoopExit {
                def_index,
                block,
                target,
                span,
            });
        }
        if target >= block_count {
            return Err(IrError::InvalidJumpTarget {
                def_index,
                block,
                target,
                block_count,
            });
        }
        let _ = &func.blocks[target as usize];
        Ok(())
    };

    match inst {
        IrInst::Jump { target } => check(*target),
        IrInst::JumpIf {
            then_block,
            else_block,
        } => {
            check(*then_block)?;
            check(*else_block)
        }
        _ => Ok(()),
    }
}

fn module_for_def(typed: &TypedProgram, def: crate::resolver::DefId) -> u32 {
    typed
        .resolved
        .defs
        .get(def.index() as usize)
        .map_or(0, |d| d.module)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::ir::{IrBasicBlock, IrFunctionId, SpannedInst};
    use crate::resolver::DefId;
    use crate::typeck::TypeId;
    use phx_diagnostics::Span;

    fn span_inst(inst: IrInst) -> SpannedInst {
        SpannedInst::new(Span::new(0, 1), inst)
    }

    fn minimal_func(blocks: Vec<IrBasicBlock>) -> IrFunction {
        IrFunction {
            id: IrFunctionId::from_raw(0),
            def: DefId::from_raw(1),
            params: Vec::new(),
            return_type: TypeId::from_raw(0),
            local_count: 0,
            blocks,
        }
    }

    fn stub_typed() -> TypedProgram {
        let source = r"main :: () => {
    const a: s32 = 5;
    const b: s32 = 7;
    const sum: s32 = a + b;
    const ok: bool = sum == 12;
};
";
        crate::compile_source(source, Some(std::path::Path::new("sample.phx")))
            .expect("compile sample fixture")
            .typed
    }

    #[test]
    fn missing_terminator_rejected() {
        let func = minimal_func(vec![IrBasicBlock {
            insts: vec![span_inst(IrInst::Pop)],
        }]);
        match validate_structure(&func) {
            Err(IrError::MissingTerminator { block: 0, .. }) => {}
            other => panic!("expected MissingTerminator, got {other:?}"),
        }
    }

    #[test]
    fn invalid_jump_target_rejected() {
        let func = minimal_func(vec![IrBasicBlock {
            insts: vec![span_inst(IrInst::Jump { target: 99 })],
        }]);
        match validate_structure(&func) {
            Err(IrError::InvalidJumpTarget { target: 99, .. }) => {}
            other => panic!("expected InvalidJumpTarget, got {other:?}"),
        }
    }

    #[test]
    fn inst_after_terminator_rejected() {
        let func = minimal_func(vec![IrBasicBlock {
            insts: vec![
                span_inst(IrInst::Return {
                    ty: TypeId::from_raw(0),
                }),
                span_inst(IrInst::Pop),
            ],
        }]);
        match validate_structure(&func) {
            Err(IrError::InstructionAfterTerminator { inst_index: 1, .. }) => {}
            other => panic!("expected InstructionAfterTerminator, got {other:?}"),
        }
    }

    #[test]
    fn join_depth_mismatch_rejected() {
        let func = minimal_func(vec![
            IrBasicBlock {
                insts: vec![
                    span_inst(IrInst::Const {
                        index: 0,
                        ty: TypeId::from_raw(0),
                        prim_kind: 0,
                    }),
                    span_inst(IrInst::JumpIf {
                        then_block: 1,
                        else_block: 2,
                    }),
                ],
            },
            IrBasicBlock {
                insts: vec![span_inst(IrInst::Jump { target: 3 })],
            },
            IrBasicBlock {
                insts: vec![
                    span_inst(IrInst::Const {
                        index: 0,
                        ty: TypeId::from_raw(0),
                        prim_kind: 0,
                    }),
                    span_inst(IrInst::Jump { target: 3 }),
                ],
            },
            IrBasicBlock {
                insts: vec![span_inst(IrInst::Return {
                    ty: TypeId::from_raw(0),
                })],
            },
        ]);
        let typed = stub_typed();
        match validate_function(&func, &typed) {
            Err(IrError::JoinDepthMismatch { block: 3, .. }) => {}
            other => panic!("expected JoinDepthMismatch at block 3, got {other:?}"),
        }
    }

    #[test]
    fn unpatched_loop_exit_rejected() {
        let func = minimal_func(vec![IrBasicBlock {
            insts: vec![span_inst(IrInst::Jump {
                target: LOOP_EXIT_TARGET_BASE,
            })],
        }]);
        match validate_structure(&func) {
            Err(IrError::UnpatchedLoopExit { .. }) => {}
            other => panic!("expected UnpatchedLoopExit, got {other:?}"),
        }
    }

    #[test]
    fn validation_enabled_true_in_debug_builds() {
        assert!(validation_enabled());
    }

    #[test]
    fn well_formed_lowered_ir_passes_sample() {
        let typed = stub_typed();
        let ir = crate::lower::lower(&typed).expect("lower sample");
        validate_ir(&ir, &typed).expect("validate sample IR");
    }
}

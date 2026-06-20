//! CFG-aware operand stack depth analysis (shared by verifier and codegen).
//!
//! Linear instruction order is not control-flow order: short-circuit `&&` / `||` place
//! successor blocks after unrelated paths, so depth must be tracked per block entry.
//!
//! ## Owning pass
//!
//! Called from [`crate::verify`] during per-function body checks. Codegen may use
//! [`return_stack_depth`] when emitting [`crate::Opcode::Return`] and relies on the same depth
//! rules via [`crate::stack_effect`].
//!
//! ## Inputs and outputs
//!
//! - **In** — decoded instruction stream, block-entry offsets, callee arity map, required return
//!   depth from the function's return type.
//! - **Out** — [`StackFlowSummary::max_depth`] on success, or [`StackFlowError`] when a path
//!   underflows, join depths disagree, or `RETURN` depth mismatches the return type.
//!
//! ## Public API
//!
//! - [`analyze_stack_cfg`] — CFG worklist simulation from offset `0`.
//! - [`return_stack_depth`] — operand-stack cells required at `RETURN` for a type-table id.

use std::collections::{HashMap, HashSet, VecDeque};

use super::instr::Instruction;
use super::opcode::Opcode;
use super::stack_effect::apply_stack_effect;
use super::types::{TypeKind, TypeTable};

/// Stack analysis failure (mapped to [`crate::verify::VerifyError`] by the verifier).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackFlowError {
    /// Effect would pop below the depth at this instruction.
    Underflow {
        /// Offset of the offending instruction within the function body.
        offset: u32,
    },
    /// Two control-flow predecessors require different stack depths at the same block entry.
    JoinDepthMismatch {
        /// Block entry offset (instruction boundary).
        offset: u32,
        /// Depth already recorded from another predecessor.
        expected: u32,
        /// Depth from the conflicting predecessor.
        found: u32,
    },
    /// [`Opcode::Return`] reached with the wrong operand-stack depth.
    ReturnDepthMismatch {
        /// Offset of the return instruction.
        offset: u32,
        /// Required depth from the function's return type.
        expected: u32,
        /// Observed depth at the return.
        found: u32,
    },
    /// [`Opcode::Call`] targets a function id not in the module table.
    InvalidCallTarget {
        /// Offset of the call instruction.
        offset: u32,
        /// Callee function id operand.
        callee: u32,
    },
}

/// Result of a successful CFG stack simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackFlowSummary {
    /// Maximum operand stack depth along any reachable path.
    pub max_depth: u32,
}

/// Decoded instruction at a relative offset within a function body.
pub type DecodedInst = (u32, Instruction);

/// Operand-stack cells required at [`Opcode::Return`] for a function return type id.
#[must_use]
pub fn return_stack_depth(return_type_id: u32, types: &TypeTable) -> u32 {
    if return_type_id == 0 {
        return 0;
    }
    match types.records.iter().find(|r| r.type_id == return_type_id) {
        Some(record) if record.kind == TypeKind::Unit => 0,
        Some(_) | None => 1,
    }
}

/// Simulates stack depth over the function CFG starting at offset `0` with depth `0`.
///
/// Block entries are instruction boundaries in `inst_starts`. Unconditional and
/// conditional branches must target instruction boundaries (validated separately).
///
/// # Errors
///
/// Returns [`StackFlowError::Underflow`] when an instruction would pop below the
/// current depth, [`StackFlowError::JoinDepthMismatch`] when predecessors disagree,
/// or [`StackFlowError::ReturnDepthMismatch`] when `RETURN` depth != `return_depth`.
#[allow(clippy::implicit_hasher)]
pub fn analyze_stack_cfg(
    instructions: &[DecodedInst],
    inst_starts: &HashSet<u32>,
    fn_arity: &HashMap<u32, u16>,
    return_depth: u32,
) -> Result<StackFlowSummary, StackFlowError> {
    if instructions.is_empty() {
        return Ok(StackFlowSummary { max_depth: 0 });
    }

    let mut by_offset: HashMap<u32, usize> = HashMap::new();
    for (i, (rel, _)) in instructions.iter().enumerate() {
        by_offset.insert(*rel, i);
    }

    if !inst_starts.contains(&0) {
        return Err(StackFlowError::Underflow { offset: 0 });
    }

    let mut entry_depth: HashMap<u32, u32> = HashMap::new();
    let mut max_depth = 0u32;
    let mut worklist = VecDeque::from([0u32]);
    entry_depth.insert(0, 0);

    while let Some(block_start) = worklist.pop_front() {
        let start_idx = *by_offset
            .get(&block_start)
            .ok_or(StackFlowError::Underflow {
                offset: block_start,
            })?;
        let mut depth = *entry_depth
            .get(&block_start)
            .ok_or(StackFlowError::Underflow {
                offset: block_start,
            })?;
        max_depth = max_depth.max(depth);

        let mut idx = start_idx;
        loop {
            let (rel, inst) = &instructions[idx];
            let call_arity = if inst.opcode == Opcode::Call {
                let callee = inst.operands.first().copied().unwrap_or(0);
                let Some(arity) = fn_arity.get(&callee) else {
                    return Err(StackFlowError::InvalidCallTarget {
                        offset: *rel,
                        callee,
                    });
                };
                Some(*arity)
            } else if inst.opcode == Opcode::CallIndirect {
                Some(u16::try_from(inst.operands.first().copied().unwrap_or(0)).unwrap_or(u16::MAX))
            } else {
                None
            };
            let field_count = match inst.opcode {
                Opcode::MakeStruct => Some(inst.operands.get(1).copied().unwrap_or(0)),
                Opcode::MakeEnum => Some(inst.operands.get(2).copied().unwrap_or(0)),
                Opcode::MakeTuple | Opcode::MakeArray => inst.operands.first().copied(),
                _ => None,
            };

            apply_stack_effect(inst.opcode, &mut depth, call_arity, field_count)
                .map_err(|_| StackFlowError::Underflow { offset: *rel })?;
            max_depth = max_depth.max(depth);

            if inst.opcode == Opcode::Return && depth != return_depth {
                return Err(StackFlowError::ReturnDepthMismatch {
                    offset: *rel,
                    expected: return_depth,
                    found: depth,
                });
            }

            let successors = terminators_successors(inst, instructions.get(idx + 1));
            if !successors.is_empty() {
                for target in successors {
                    enqueue_entry(target, depth, &mut entry_depth, &mut worklist, inst_starts)?;
                }
                break;
            }

            if matches!(inst.opcode, Opcode::Return | Opcode::Trap) {
                break;
            }

            if idx + 1 >= instructions.len() {
                break;
            }
            let next_rel = instructions[idx + 1].0;
            if inst_starts.contains(&next_rel) {
                enqueue_entry(
                    next_rel,
                    depth,
                    &mut entry_depth,
                    &mut worklist,
                    inst_starts,
                )?;
                break;
            }
            idx += 1;
        }
    }

    Ok(StackFlowSummary { max_depth })
}

fn enqueue_entry(
    target: u32,
    depth: u32,
    entry_depth: &mut HashMap<u32, u32>,
    worklist: &mut VecDeque<u32>,
    inst_starts: &HashSet<u32>,
) -> Result<(), StackFlowError> {
    if !inst_starts.contains(&target) {
        return Ok(());
    }
    match entry_depth.get(&target) {
        None => {
            entry_depth.insert(target, depth);
            worklist.push_back(target);
        }
        Some(&existing) if existing == depth => {}
        Some(&existing) => {
            return Err(StackFlowError::JoinDepthMismatch {
                offset: target,
                expected: existing,
                found: depth,
            });
        }
    }
    Ok(())
}

fn conditional_branch_successors(
    inst: &Instruction,
    next: Option<&(u32, Instruction)>,
) -> Vec<u32> {
    let Some(branch) = inst.operands.first().copied() else {
        return Vec::new();
    };
    let alternate = next.and_then(|(fallthrough_rel, n)| {
        if n.opcode == Opcode::Jump {
            n.operands.first().copied()
        } else {
            Some(*fallthrough_rel)
        }
    });
    match alternate {
        Some(alt) => vec![branch, alt],
        None => vec![branch],
    }
}

fn terminators_successors(inst: &Instruction, next: Option<&(u32, Instruction)>) -> Vec<u32> {
    match inst.opcode {
        Opcode::Jump => inst.operands.first().copied().into_iter().collect(),
        Opcode::JumpIfTrue | Opcode::JumpIfFalse => conditional_branch_successors(inst, next),
        Opcode::Const
        | Opcode::LoadLocal
        | Opcode::StoreLocal
        | Opcode::Pop
        | Opcode::Add
        | Opcode::Sub
        | Opcode::Mul
        | Opcode::Div
        | Opcode::Eq
        | Opcode::Lt
        | Opcode::Return
        | Opcode::Call
        | Opcode::MakeStruct
        | Opcode::MakeEnum
        | Opcode::GetField
        | Opcode::SetField
        | Opcode::MatchTag
        | Opcode::Cast
        | Opcode::Mod
        | Opcode::Pow
        | Opcode::Neg
        | Opcode::Not
        | Opcode::BitNot
        | Opcode::BitAnd
        | Opcode::BitOr
        | Opcode::BitXor
        | Opcode::Shl
        | Opcode::Shr
        | Opcode::Ne
        | Opcode::Le
        | Opcode::Ge
        | Opcode::MakeTuple
        | Opcode::MakeArray
        | Opcode::Index
        | Opcode::Trap
        | Opcode::Alloc
        | Opcode::PtrLoad
        | Opcode::PtrStore
        | Opcode::MakeSlice
        | Opcode::AddressOfLocal
        | Opcode::MakeStr
        | Opcode::StrAsSlice
        | Opcode::SliceLen
        | Opcode::MakeFnPtr
        | Opcode::CallIndirect
        | Opcode::LoadAggViaLocalPtr
        | Opcode::MakeSliceFromPtr
        | Opcode::Free
        | Opcode::IndexStore => Vec::new(),
    }
}

#[cfg(test)]
#[allow(clippy::cast_lossless, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::cast::PrimitiveKind;
    use crate::instr::Instruction;

    #[test]
    fn jump_if_false_branch_underflow_rejected() {
        let push_true = Instruction {
            opcode: Opcode::Const,
            operands: vec![0, PrimitiveKind::Bool.as_u8() as u32],
        };
        let jump_if_false = Instruction {
            opcode: Opcode::JumpIfFalse,
            operands: vec![0],
        };
        let ret = Instruction {
            opcode: Opcode::Return,
            operands: vec![],
        };
        let add = Instruction {
            opcode: Opcode::Add,
            operands: vec![u32::from(PrimitiveKind::S32.as_u8())],
        };

        let mut code = Vec::new();
        code.extend(push_true.encode().expect("encode"));
        code.extend(jump_if_false.encode().expect("encode"));
        let fallthrough = u32::try_from(code.len()).expect("offset");
        code.extend(ret.encode().expect("encode"));
        let branch = u32::try_from(code.len()).expect("offset");
        code.extend(add.encode().expect("encode"));
        let add_off = branch;
        code.extend(ret.encode().expect("encode"));

        let mut instructions = Vec::new();
        let mut off = 0usize;
        while off < code.len() {
            let (inst, next) = Instruction::decode_at(&code, off).expect("decode");
            instructions.push((u32::try_from(off).expect("offset"), inst));
            off = next;
        }
        instructions[1].1.operands[0] = branch;

        let inst_starts: HashSet<u32> = [0, branch, fallthrough].into_iter().collect();
        match analyze_stack_cfg(&instructions, &inst_starts, &HashMap::new(), 0) {
            Err(StackFlowError::Underflow { offset }) if offset == add_off => {}
            other => panic!("expected Underflow at {add_off}, got {other:?}"),
        }
    }

    #[test]
    fn jump_if_true_fallthrough_underflow_rejected() {
        let push_false = Instruction {
            opcode: Opcode::Const,
            operands: vec![0, PrimitiveKind::Bool.as_u8() as u32],
        };
        let jump_if_true = Instruction {
            opcode: Opcode::JumpIfTrue,
            operands: vec![0],
        };
        let add = Instruction {
            opcode: Opcode::Add,
            operands: vec![u32::from(PrimitiveKind::S32.as_u8())],
        };
        let ret = Instruction {
            opcode: Opcode::Return,
            operands: vec![],
        };

        let mut code = Vec::new();
        code.extend(push_false.encode().expect("encode"));
        code.extend(jump_if_true.encode().expect("encode"));
        let add_off = u32::try_from(code.len()).expect("offset");
        code.extend(add.encode().expect("encode"));
        code.extend(ret.encode().expect("encode"));
        let branch = u32::try_from(code.len()).expect("offset");
        code.extend(ret.encode().expect("encode"));

        let mut instructions = Vec::new();
        let mut off = 0usize;
        while off < code.len() {
            let (inst, next) = Instruction::decode_at(&code, off).expect("decode");
            instructions.push((u32::try_from(off).expect("offset"), inst));
            off = next;
        }
        instructions[1].1.operands[0] = branch;

        let inst_starts: HashSet<u32> = instructions.iter().map(|(rel, _)| *rel).collect();
        match analyze_stack_cfg(&instructions, &inst_starts, &HashMap::new(), 0) {
            Err(StackFlowError::Underflow { offset }) if offset == add_off => {}
            other => panic!("expected Underflow at {add_off}, got {other:?}"),
        }
    }

    #[test]
    fn short_circuit_paths_use_independent_entry_depth() {
        // Entry: const, JumpIfTrue(then) + Jump(else); then/else each push one bool; merge returns.
        let entry_const = Instruction {
            opcode: Opcode::Const,
            operands: vec![0, PrimitiveKind::Bool.as_u8() as u32],
        };
        let jump_if = Instruction {
            opcode: Opcode::JumpIfTrue,
            operands: vec![0], // patched below
        };
        let jump_else = Instruction {
            opcode: Opcode::Jump,
            operands: vec![0],
        };
        let branch_const = Instruction {
            opcode: Opcode::Const,
            operands: vec![1, PrimitiveKind::Bool.as_u8() as u32],
        };
        let jump_merge = Instruction {
            opcode: Opcode::Jump,
            operands: vec![0],
        };
        let ret = Instruction {
            opcode: Opcode::Return,
            operands: vec![],
        };

        let mut code = Vec::new();
        let a = 0u32;
        code.extend(entry_const.encode().expect("encode"));
        let jump_if_off = u32::try_from(code.len()).expect("offset");
        code.extend(jump_if.encode().expect("encode"));
        code.extend(jump_else.encode().expect("encode"));
        let b = u32::try_from(code.len()).expect("offset");
        code.extend(branch_const.encode().expect("encode"));
        let b_jump_off = u32::try_from(code.len()).expect("offset");
        code.extend(jump_merge.encode().expect("encode"));
        let c = u32::try_from(code.len()).expect("offset");
        code.extend(branch_const.encode().expect("encode"));
        let c_jump_off = u32::try_from(code.len()).expect("offset");
        code.extend(jump_merge.encode().expect("encode"));
        let d = u32::try_from(code.len()).expect("offset");
        code.extend(ret.encode().expect("encode"));

        let mut instructions = Vec::new();
        let mut off = 0usize;
        while off < code.len() {
            let (inst, next) = Instruction::decode_at(&code, off).expect("decode");
            let rel = u32::try_from(off).expect("offset");
            instructions.push((rel, inst));
            off = next;
        }

        let mut patch = |idx: usize, target: u32| {
            instructions[idx].1.operands[0] = target;
        };
        patch(1, c);
        patch(2, b);
        patch(4, d);
        patch(6, d);

        let _ = (jump_if_off, b_jump_off, c_jump_off);
        let inst_starts: HashSet<u32> = [a, b, c, d].into_iter().collect();
        let summary =
            analyze_stack_cfg(&instructions, &inst_starts, &HashMap::new(), 1).expect("cfg");
        assert_eq!(summary.max_depth, 1);
    }

    #[test]
    fn return_depth_mismatch_rejected() {
        let push = Instruction {
            opcode: Opcode::Const,
            operands: vec![0, PrimitiveKind::S32.as_u8() as u32],
        };
        let ret = Instruction {
            opcode: Opcode::Return,
            operands: vec![],
        };
        let mut code = Vec::new();
        code.extend(push.encode().expect("encode"));
        let ret_off = u32::try_from(code.len()).expect("offset");
        code.extend(ret.encode().expect("encode"));

        let mut instructions = Vec::new();
        let mut off = 0usize;
        while off < code.len() {
            let (inst, next) = Instruction::decode_at(&code, off).expect("decode");
            instructions.push((u32::try_from(off).expect("offset"), inst));
            off = next;
        }

        let inst_starts: HashSet<u32> = [0].into_iter().collect();
        match analyze_stack_cfg(&instructions, &inst_starts, &HashMap::new(), 0) {
            Err(StackFlowError::ReturnDepthMismatch {
                offset,
                expected: 0,
                found: 1,
            }) if offset == ret_off => {}
            other => panic!("expected ReturnDepthMismatch at {ret_off}, got {other:?}"),
        }
    }
}

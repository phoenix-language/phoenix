//! CFG-aware operand stack depth analysis (shared by verifier and codegen).
//!
//! Linear instruction order is not control-flow order: short-circuit `&&` / `||` place
//! successor blocks after unrelated paths, so depth must be tracked per block entry.

use std::collections::{HashMap, HashSet, VecDeque};

use super::instr::Instruction;
use super::opcode::Opcode;
use super::stack_effect::apply_stack_effect;

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

/// Simulates stack depth over the function CFG starting at offset `0` with depth `0`.
///
/// Block entries are instruction boundaries in `inst_starts`. Unconditional and
/// conditional branches must target instruction boundaries (validated separately).
///
/// # Errors
///
/// Returns [`StackFlowError::Underflow`] when an instruction would pop below the
/// current depth, or [`StackFlowError::JoinDepthMismatch`] when predecessors disagree.
#[allow(clippy::implicit_hasher)]
pub fn analyze_stack_cfg(
    instructions: &[DecodedInst],
    inst_starts: &HashSet<u32>,
    fn_arity: &HashMap<u32, u16>,
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

fn terminators_successors(inst: &Instruction, next: Option<&(u32, Instruction)>) -> Vec<u32> {
    match inst.opcode {
        Opcode::Jump => inst.operands.first().copied().into_iter().collect(),
        Opcode::JumpIfTrue => {
            let then = inst.operands.first().copied();
            let else_target = next.and_then(|(_, n)| {
                if n.opcode == Opcode::Jump {
                    n.operands.first().copied()
                } else {
                    None
                }
            });
            match (then, else_target) {
                (Some(t), Some(e)) => vec![t, e],
                (Some(t), None) => vec![t],
                _ => Vec::new(),
            }
        }
        Opcode::Return | Opcode::Trap => Vec::new(),
        #[allow(clippy::match_same_arms)]
        _ => Vec::new(),
    }
}

#[cfg(test)]
#[allow(clippy::cast_lossless, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::cast::PrimitiveKind;
    use crate::instr::Instruction;

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
        code.extend(entry_const.encode());
        let jump_if_off = u32::try_from(code.len()).expect("offset");
        code.extend(jump_if.encode());
        code.extend(jump_else.encode());
        let b = u32::try_from(code.len()).expect("offset");
        code.extend(branch_const.encode());
        let b_jump_off = u32::try_from(code.len()).expect("offset");
        code.extend(jump_merge.encode());
        let c = u32::try_from(code.len()).expect("offset");
        code.extend(branch_const.encode());
        let c_jump_off = u32::try_from(code.len()).expect("offset");
        code.extend(jump_merge.encode());
        let d = u32::try_from(code.len()).expect("offset");
        code.extend(ret.encode());

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
        let summary = analyze_stack_cfg(&instructions, &inst_starts, &HashMap::new()).expect("cfg");
        assert_eq!(summary.max_depth, 1);
    }
}

//! Shared lowering context: expression-type cursor and CFG builder.

use phx_diagnostics::Span;
use phx_syntax::Symbol;

use crate::ir::{IrBasicBlock, IrInst};
use crate::resolver::{DefId, ResolutionKey, ResolvedProgram};
use crate::typeck::{ExprId, FunctionLayout, LocalSlot, Ty, TypeId, TypedProgram};

/// Jump target placeholder for a loop exit not yet allocated (`0xF000_0000 + slot`).
pub const LOOP_EXIT_TARGET_BASE: u32 = 0xF000_0000;

/// Jump targets for `break` / `continue` in the innermost active loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopLabels {
    /// Index into [`LowerCtx::pending_loop_exits`]; resolved after the loop body.
    pub exit_slot: usize,
    /// Block where the next iteration begins (`while`: condition; `loop`: body head).
    pub continue_target: u32,
}

/// Mutable state while lowering one function body.
pub struct LowerCtx<'a> {
    /// Typed program (AST, types, resolutions).
    pub typed: &'a TypedProgram,
    /// Current function slot layout.
    pub layout: &'a FunctionLayout,
    /// Next [`ExprId`] raw index (must match typeck visit order).
    pub next_expr: u32,
    /// Basic blocks (block 0 is entry).
    pub blocks: Vec<IrBasicBlock>,
    /// Block index receiving non-terminator instructions.
    pub current: u32,
    /// Stack of active loops (innermost last).
    pub loop_stack: Vec<LoopLabels>,
    /// Next index into [`FunctionLayout::match_temp_slots`].
    pub match_temp_index: usize,
    /// Loop exit blocks allocated after loop bodies (for `break` / `while` exit).
    pub pending_loop_exits: Vec<Option<u32>>,
}

impl<'a> LowerCtx<'a> {
    /// Creates a context with a single empty entry block.
    #[must_use]
    pub fn new(typed: &'a TypedProgram, layout: &'a FunctionLayout) -> Self {
        Self {
            typed,
            layout,
            next_expr: 0,
            blocks: vec![IrBasicBlock::new()],
            current: 0,
            loop_stack: Vec::new(),
            match_temp_index: 0,
            pending_loop_exits: Vec::new(),
        }
    }

    /// Reserves a loop exit block id to patch after the loop body is lowered.
    #[must_use]
    pub fn alloc_loop_exit_slot(&mut self) -> usize {
        let slot = self.pending_loop_exits.len();
        self.pending_loop_exits.push(None);
        slot
    }

    /// Placeholder jump target for loop exit slot `slot`.
    #[must_use]
    pub fn loop_exit_target(slot: usize) -> u32 {
        LOOP_EXIT_TARGET_BASE.saturating_add(u32::try_from(slot).unwrap_or(u32::MAX))
    }

    /// Resolves [`LOOP_EXIT_TARGET_BASE`] placeholders in `blocks`.
    pub fn patch_loop_exit_targets(
        blocks: &mut [crate::ir::IrBasicBlock],
        pending: &[Option<u32>],
    ) {
        for block in blocks {
            for inst in &mut block.insts {
                match inst {
                    crate::ir::IrInst::Jump { target } => {
                        Self::patch_one_target(target, pending);
                    }
                    crate::ir::IrInst::JumpIf {
                        then_block,
                        else_block,
                    } => {
                        Self::patch_one_target(then_block, pending);
                        Self::patch_one_target(else_block, pending);
                    }
                    _ => {}
                }
            }
        }
    }

    fn patch_one_target(target: &mut u32, pending: &[Option<u32>]) {
        if *target >= LOOP_EXIT_TARGET_BASE {
            let slot = usize::try_from(*target - LOOP_EXIT_TARGET_BASE).unwrap_or(usize::MAX);
            if let Some(exit) = pending.get(slot).and_then(|o| *o) {
                *target = exit;
            }
        }
    }

    /// Returns the next `match` scrutinee temp slot for this function.
    #[must_use]
    pub fn next_match_temp(&mut self) -> LocalSlot {
        let slot = self
            .layout
            .match_temp_slots
            .get(self.match_temp_index)
            .copied()
            .unwrap_or_else(|| LocalSlot::from_raw(0));
        self.match_temp_index += 1;
        slot
    }

    /// Records `labels` as the innermost loop for nested `break` / `continue`.
    pub fn push_loop(&mut self, labels: LoopLabels) {
        self.loop_stack.push(labels);
    }

    /// Removes the innermost loop after its body has been lowered.
    pub fn pop_loop(&mut self) {
        let _ = self.loop_stack.pop();
    }

    /// Returns jump targets for the innermost loop, if any.
    #[must_use]
    pub fn innermost_loop(&self) -> Option<LoopLabels> {
        self.loop_stack.last().copied()
    }

    /// Advances the expression cursor and returns the typeck-assigned type.
    ///
    /// Returns the typeck-assigned type for the next expression (visit order must match typeck).
    pub fn expr_ty(&mut self) -> TypeId {
        let id = ExprId::from_raw(self.next_expr);
        self.next_expr += 1;
        self.typed
            .expr_types
            .get(&id)
            .copied()
            .unwrap_or_else(|| unit_ty(self.typed))
    }

    /// Appends a non-terminator or terminator to the current block.
    pub fn emit(&mut self, inst: IrInst) {
        let block = match usize::try_from(self.current) {
            Ok(b) => b,
            Err(_) => return,
        };
        if let Some(b) = self.blocks.get_mut(block) {
            b.insts.push(inst);
        }
    }

    /// Allocates a new empty basic block and returns its index.
    #[must_use]
    pub fn fresh_block(&mut self) -> u32 {
        let id = u32::try_from(self.blocks.len()).unwrap_or(u32::MAX);
        self.blocks.push(IrBasicBlock::new());
        id
    }

    /// Switches emission to `block`.
    pub fn set_current(&mut self, block: u32) {
        self.current = block;
    }

    /// Finishes building blocks.
    #[must_use]
    pub fn into_blocks(self) -> Vec<IrBasicBlock> {
        self.blocks
    }
}

/// Looks up a use-site resolution.
#[must_use]
pub fn lookup_resolution(resolved: &ResolvedProgram, span: Span, symbol: Symbol) -> Option<DefId> {
    resolved
        .resolutions
        .get(&ResolutionKey {
            start: span.start,
            end: span.end,
            symbol,
        })
        .copied()
}

/// Returns the interned unit type id.
#[must_use]
pub fn unit_ty(typed: &TypedProgram) -> TypeId {
    use crate::typeck::Ty;
    for (i, t) in typed.types.types().iter().enumerate() {
        if t == &Ty::Unit {
            if let Ok(raw) = u32::try_from(i) {
                return TypeId::from_raw(raw);
            }
        }
    }
    TypeId::from_raw(0)
}

/// Returns the interned `bool` type id.
#[must_use]
pub fn bool_ty(typed: &TypedProgram) -> TypeId {
    use crate::typeck::Ty;
    use phx_syntax::token::Keyword;
    for (i, t) in typed.types.types().iter().enumerate() {
        if t == &Ty::Primitive(Keyword::Bool) {
            if let Ok(raw) = u32::try_from(i) {
                return TypeId::from_raw(raw);
            }
        }
    }
    TypeId::from_raw(0)
}

/// Encodes an MVP literal into a [`IrInst::Const`] index (until module const pool exists).
#[must_use]
pub fn const_index_for_literal(lit: &phx_syntax::ast::lit::Literal) -> Option<u32> {
    use phx_syntax::ast::lit::Literal;
    match lit {
        Literal::Int(i) => {
            let v = i32::try_from(i.value).ok()?;
            Some(v.cast_unsigned())
        }
        Literal::Bool(b) => Some(u32::from(*b)),
        Literal::Float(_) | Literal::ByteChar(_) | Literal::ByteString(_) => None,
        _ => None,
    }
}

/// Maps slot for symbol in layout.
#[must_use]
pub fn slot_for_symbol(layout: &FunctionLayout, symbol: Symbol) -> Option<LocalSlot> {
    layout.binding(symbol).map(|b| b.slot)
}

/// Returns the named type `def` for a struct/enum value type, if any.
#[must_use]
pub fn named_def_for_ty(typed: &TypedProgram, ty: TypeId) -> Option<DefId> {
    if let Ty::Named { def, .. } = typed.types.get(ty) {
        Some(*def)
    } else {
        None
    }
}

/// Finds a struct definition by type name symbol.
#[must_use]
pub fn struct_def_by_name(resolved: &ResolvedProgram, name: Symbol) -> Option<DefId> {
    use crate::resolver::DefKind;
    for (i, d) in resolved.defs.iter().enumerate() {
        if d.name == name && d.kind == DefKind::Struct {
            return u32::try_from(i).ok().map(DefId::from_raw);
        }
    }
    None
}

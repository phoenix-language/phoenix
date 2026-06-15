//! Shared lowering context: expression-type cursor and CFG builder.

use phx_syntax::Symbol;

use crate::ir::{IrBasicBlock, IrConst, IrInst, SpannedInst};
use crate::resolver::{DefId, ResolutionKey, ResolvedProgram};
use std::collections::HashSet;

use crate::typeck::{
    ExprId, FunctionLayout, LocalSlot, ProgramLayout, Ty, TypeId, TypedProgram,
    primitive_kind_for_type,
};
use phx_bytecode::SLOT_KIND_AGG;
use phx_bytecode::SLOT_KIND_FN_PTR;
use phx_diagnostics::{LowerBag, LowerError, Span};

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
    /// Module containing the function being lowered (for resolution keys).
    pub module: u32,
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
    /// Next index into [`FunctionLayout::for_in_plans`].
    pub for_in_index: usize,
    /// Loop exit blocks allocated after loop bodies (for `break` / `while` exit).
    pub pending_loop_exits: Vec<Option<u32>>,
    /// Module constant literals (shared across functions).
    pub constants: &'a mut Vec<IrConst>,
    /// Lowering errors for this function (merged into module bag on failure).
    pub bag: &'a mut LowerBag,
    /// Block nesting depth while lowering (mirrors typeck scope depth).
    pub scope_depth: u32,
    /// Layout scope depth at each active loop body entry (before block `enter_scope`).
    pub loop_body_scope_depths: Vec<u32>,
    /// Drop slots already emitted (avoids duplicate glue on branch merge).
    pub emitted_drop_slots: HashSet<LocalSlot>,
    /// Current source site for internal lowering diagnostics.
    pub site: Span,
}

impl<'a> LowerCtx<'a> {
    /// Creates a context with a single empty entry block.
    #[must_use]
    pub fn new(
        typed: &'a TypedProgram,
        module: u32,
        layout: &'a FunctionLayout,
        constants: &'a mut Vec<IrConst>,
        bag: &'a mut LowerBag,
        site: Span,
    ) -> Self {
        Self {
            typed,
            module,
            layout,
            next_expr: layout.expr_start,
            blocks: vec![IrBasicBlock::new()],
            current: 0,
            loop_stack: Vec::new(),
            match_temp_index: 0,
            for_in_index: 0,
            pending_loop_exits: Vec::new(),
            constants,
            bag,
            scope_depth: 0,
            loop_body_scope_depths: Vec::new(),
            emitted_drop_slots: HashSet::new(),
            site,
        }
    }

    /// Enters a nested block scope for drop-glue tracking.
    pub fn enter_scope(&mut self) {
        self.scope_depth = self.scope_depth.saturating_add(1);
    }

    /// Leaves a nested block scope, emitting drop glue for bindings at this depth.
    pub fn exit_scope(&mut self) {
        crate::lower::drop_glue::emit_scope_drops(self, self.scope_depth, self.scope_depth);
        self.scope_depth = self.scope_depth.saturating_sub(1);
    }

    /// Records an unresolved call at `span` and skips emitting `IrInst::Call`.
    pub fn error_unresolved_callee(&mut self, span: Span) {
        self.bag
            .push(self.module, LowerError::UnresolvedCallee { span });
    }

    fn error_invalid_block(&mut self, block: u32) {
        self.bag
            .push(self.module, LowerError::InvalidBlockIndex { block });
    }

    fn error_limit_exceeded(&mut self, item: &'static str, len: usize) {
        self.bag
            .push(self.module, LowerError::LimitExceeded { item, len });
    }

    /// Appends a literal to the module pool and returns its index.
    ///
    /// On overflow, records [`LowerError::LimitExceeded`] and returns `0`; callers should
    /// abort lowering when [`LowerBag::has_errors`].
    pub fn intern_const(&mut self, lit: IrConst) -> u32 {
        if let Ok(index) = u32::try_from(self.constants.len()) {
            self.constants.push(lit);
            index
        } else {
            self.error_limit_exceeded("const_pool", self.constants.len());
            0
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
            for spanned in &mut block.insts {
                match &mut spanned.inst {
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
    /// Visit order must match typeck (`check_expr_node` pre-order). On a missing map entry for an
    /// id in [`FunctionLayout::expr_start`, `expr_end`), records [`LowerError::MissingExprType`]
    /// and returns a poison `()` type so lowering can continue collecting errors.
    pub fn expr_ty(&mut self) -> TypeId {
        let raw = self.next_expr;
        self.next_expr = self.next_expr.saturating_add(1);
        self.lookup_expr_type(raw)
    }

    /// Returns the typeck-assigned type for the next expression without advancing the cursor.
    pub fn peek_next_expr_ty(&mut self) -> TypeId {
        self.lookup_expr_type(self.next_expr)
    }

    /// Verifies the expression cursor ended at `layout.expr_end`.
    pub fn finish_expr_cursor(&mut self) {
        if self.next_expr != self.layout.expr_end {
            self.bag.push(
                self.module,
                LowerError::ExprCursorDrift {
                    expected: self.layout.expr_end,
                    found: self.next_expr,
                    span: self.site,
                },
            );
        }
    }

    fn lookup_expr_type(&mut self, raw: u32) -> TypeId {
        let id = ExprId::from_raw(raw);
        if let Some(ty) = self.typed.expr_types.get(&id) {
            *ty
        } else {
            if raw >= self.layout.expr_start && raw < self.layout.expr_end {
                self.bag.push(
                    self.module,
                    LowerError::MissingExprType {
                        expr_id: raw,
                        span: self.site,
                    },
                );
            }
            unit_ty(self.typed)
        }
    }

    /// Returns a compiler intrinsic site only when `expr_id` belongs to this function's typeck range.
    ///
    /// Generic impl templates may omit body typeck (`expr_start == expr_end`); without this guard,
    /// lowering would mis-apply intrinsic sites from other functions that reuse the same expr ids.
    pub fn intrinsic_site_for_expr(
        &self,
        expr_id: crate::typeck::ExprId,
    ) -> Option<crate::typeck::IntrinsicSite> {
        let raw = expr_id.index();
        if raw < self.layout.expr_start || raw >= self.layout.expr_end {
            return None;
        }
        self.typed.intrinsic_call_sites.get(&expr_id).copied()
    }

    /// Appends a non-terminator or terminator to the current block.
    pub fn emit(&mut self, span: Span, inst: IrInst) {
        let block = if let Ok(b) = usize::try_from(self.current) {
            b
        } else {
            self.error_invalid_block(self.current);
            return;
        };
        if let Some(b) = self.blocks.get_mut(block) {
            b.insts.push(SpannedInst::new(span, inst));
        } else {
            self.error_invalid_block(self.current);
        }
    }

    /// Appends an instruction at [`Self::site`].
    pub fn emit_here(&mut self, inst: IrInst) {
        self.emit(self.site, inst);
    }

    /// Updates the current lowering site for internal error attribution.
    pub fn set_site(&mut self, span: Span) {
        self.site = span;
    }

    /// Allocates a new empty basic block and returns its index.
    ///
    /// On overflow, records [`LowerError::LimitExceeded`] and returns `0`.
    #[must_use]
    pub fn fresh_block(&mut self) -> u32 {
        if let Ok(id) = u32::try_from(self.blocks.len()) {
            self.blocks.push(IrBasicBlock::new());
            id
        } else {
            self.error_limit_exceeded("basic_blocks", self.blocks.len());
            0
        }
    }

    /// Switches emission to `block`.
    pub fn set_current(&mut self, block: u32) {
        let index = if let Ok(i) = usize::try_from(block) {
            i
        } else {
            self.error_invalid_block(block);
            return;
        };
        if self.blocks.get(index).is_some() {
            self.current = block;
        } else {
            self.error_invalid_block(block);
        }
    }

    /// Finishes building blocks.
    #[must_use]
    pub fn into_blocks(self) -> Vec<IrBasicBlock> {
        self.blocks
    }
}

/// Looks up a use-site resolution.
#[must_use]
pub fn lookup_resolution(
    resolved: &ResolvedProgram,
    module: u32,
    node_id: phx_syntax::AstNodeId,
) -> Option<DefId> {
    resolved
        .resolutions
        .get(&ResolutionKey { module, node_id })
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

/// Wire primitive kind byte for `ty` (aggregate types use [`SLOT_KIND_AGG`]).
#[must_use]
pub fn prim_kind_byte(typed: &TypedProgram, ty: TypeId) -> u8 {
    if matches!(typed.types.get(ty), Ty::Fn { .. }) {
        SLOT_KIND_FN_PTR
    } else if matches!(typed.types.get(ty), Ty::Ptr { .. } | Ty::Ref { .. }) {
        phx_bytecode::PrimitiveKind::U64.as_u8()
    } else {
        primitive_kind_for_type(&typed.types, ty)
            .map_or(SLOT_KIND_AGG, phx_bytecode::PrimitiveKind::as_u8)
    }
}

/// Maps slot for symbol in layout.
#[must_use]
pub fn slot_for_symbol(layout: &FunctionLayout, symbol: Symbol) -> Option<LocalSlot> {
    layout.binding(symbol).map(|b| b.slot)
}

/// Resolves bytecode struct `type_id` and field layout for a struct literal.
#[must_use]
pub fn struct_lit_layout_ops<'a>(
    layout: &'a ProgramLayout,
    type_def: DefId,
    args: &[TypeId],
) -> Option<(u32, &'a crate::typeck::StructLayout)> {
    if let Some(sl) = layout.struct_layout(type_def, args) {
        let type_id = layout.type_id_for_named(type_def, args)?;
        return Some((type_id, sl));
    }
    if args.is_empty() {
        return None;
    }
    let mut matches: Vec<_> = layout
        .specialized_structs
        .iter()
        .filter(|(key, _)| key.base == type_def && key.args.len() == args.len())
        .collect();
    matches.sort_by_key(|(key, _)| key.args.first().map_or(0, |t| t.index()));
    if matches.len() == 1 {
        let (key, sl) = matches[0];
        let type_id = layout.specialized_type_ids.get(key).copied()?;
        return Some((type_id, sl));
    }
    None
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::compile_source;
    use phx_diagnostics::LowerError;
    use phx_syntax::Span;
    use std::path::Path;

    const TEST_SITE: Span = Span::new(0, 1);

    fn main_layout(typed: &TypedProgram) -> &FunctionLayout {
        typed
            .functions
            .iter()
            .find(|layout| typed.entry == Some(layout.def))
            .expect("main layout")
    }

    fn main_module(typed: &TypedProgram) -> u32 {
        let layout = main_layout(typed);
        typed
            .resolved
            .defs
            .get(layout.def.index() as usize)
            .expect("main def")
            .module
    }

    #[test]
    fn expr_ty_missing_in_range_records_error() {
        let source = "main :: () => { const x: s32 = 1; };";
        let mut unit = compile_source(source, None).expect("compile");
        let entry = unit.typed.entry;
        let (module, expected_id) = {
            let layout = main_layout(&unit.typed);
            (main_module(&unit.typed), layout.expr_start)
        };
        unit.typed.expr_types.clear();
        let layout = unit
            .typed
            .functions
            .iter()
            .find(|layout| entry == Some(layout.def))
            .expect("main layout");
        let mut constants = Vec::new();
        let mut bag = LowerBag::new();
        let mut ctx = LowerCtx::new(
            &unit.typed,
            module,
            layout,
            &mut constants,
            &mut bag,
            TEST_SITE,
        );
        let _ = ctx.expr_ty();
        assert!(
            bag.errors()
                .iter()
                .any(|e| matches!(e.error, LowerError::MissingExprType { expr_id, .. } if expr_id == expected_id)),
            "expected MissingExprType for id {expected_id}, got {bag:?}"
        );
    }

    #[test]
    fn finish_expr_cursor_detects_drift() {
        let source = "main :: () => { };";
        let unit = compile_source(source, None).expect("compile");
        let layout = main_layout(&unit.typed);
        let module = main_module(&unit.typed);
        let mut constants = Vec::new();
        let mut bag = LowerBag::new();
        let mut ctx = LowerCtx::new(
            &unit.typed,
            module,
            layout,
            &mut constants,
            &mut bag,
            TEST_SITE,
        );
        ctx.next_expr = layout.expr_end.saturating_add(2);
        ctx.finish_expr_cursor();
        assert!(
            bag.errors().iter().any(|e| {
                matches!(
                    e.error,
                    LowerError::ExprCursorDrift {
                        expected,
                        found,
                        ..
                    } if expected == layout.expr_end && found == layout.expr_end.saturating_add(2)
                )
            }),
            "expected ExprCursorDrift, got {bag:?}"
        );
    }

    #[test]
    fn lower_generic_impl_method_balances_expr_cursor() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/cli/fixtures/generic_impl_method.phx");
        let unit = crate::check_file(&path).expect("check");
        let ir = crate::lower::lower(&unit.typed).expect("lower");
        assert!(
            ir.functions.iter().any(|f| {
                unit.typed
                    .functions
                    .iter()
                    .find(|layout| layout.def == f.def)
                    .is_some_and(|layout| layout.expr_start < layout.expr_end)
            }),
            "expected specialized function with typechecked expression range"
        );
    }

    #[test]
    fn method_call_sites_specialize_per_receiver_type() {
        const SOURCE: &str = r"
Box :: <t> struct { v: t };

Box :: <t> impl {
  get :: () => t { self.v };
};

main :: () => {
  const a = Box :: <s32> { v: 1 };
  const b = Box :: <u32> { v: 2u };
  const x = a.get();
  const y = b.get();
  const _x = x;
  const _y = y;
};
";
        let unit = compile_source(SOURCE, None).expect("compile");
        let callees: Vec<_> = unit
            .typed
            .method_call_sites
            .values()
            .map(|meta| meta.template)
            .collect();
        assert_eq!(
            callees.len(),
            2,
            "expected two method call sites, got {callees:?}"
        );
        assert_ne!(
            callees[0], callees[1],
            "s32 and u32 calls must monomorphize to distinct callees"
        );
        assert!(
            callees
                .iter()
                .all(|def| unit.typed.specialized_from.contains_key(def)),
            "expected specialized callees, got {callees:?}"
        );
    }

    #[test]
    fn emit_invalid_block_records_error() {
        let source = "main :: () => { };";
        let unit = compile_source(source, None).expect("compile");
        let layout = main_layout(&unit.typed);
        let module = main_module(&unit.typed);
        let mut constants = Vec::new();
        let mut bag = LowerBag::new();
        let mut ctx = LowerCtx::new(
            &unit.typed,
            module,
            layout,
            &mut constants,
            &mut bag,
            TEST_SITE,
        );
        ctx.current = 99;
        ctx.emit(TEST_SITE, IrInst::Jump { target: 0 });
        let insts_empty = ctx.blocks[0].insts.is_empty();
        drop(ctx);
        assert!(
            bag.errors()
                .iter()
                .any(|e| { matches!(e.error, LowerError::InvalidBlockIndex { block: 99 }) }),
            "expected InvalidBlockIndex for block 99, got {bag:?}"
        );
        assert!(
            insts_empty,
            "instruction must not be emitted to a valid block"
        );
    }

    #[test]
    fn set_current_invalid_block_records_error() {
        let source = "main :: () => { };";
        let unit = compile_source(source, None).expect("compile");
        let layout = main_layout(&unit.typed);
        let module = main_module(&unit.typed);
        let mut constants = Vec::new();
        let mut bag = LowerBag::new();
        let mut ctx = LowerCtx::new(
            &unit.typed,
            module,
            layout,
            &mut constants,
            &mut bag,
            TEST_SITE,
        );
        ctx.set_current(99);
        let current = ctx.current;
        drop(ctx);
        assert_eq!(current, 0, "current block must remain unchanged");
        assert!(
            bag.errors()
                .iter()
                .any(|e| { matches!(e.error, LowerError::InvalidBlockIndex { block: 99 }) }),
            "expected InvalidBlockIndex for block 99, got {bag:?}"
        );
    }

    #[test]
    fn generic_infer_lowers_without_cursor_drift() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/cli/fixtures/generic_infer.phx");
        let unit = crate::check_file(&path).expect("check");
        crate::lower::lower(&unit.typed).expect("lower generic_infer");
    }

    #[test]
    fn missing_method_call_site_fails_lower() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/cli/fixtures/generic_impl_method.phx");
        let mut unit = crate::check_file(&path).expect("check");
        unit.typed.method_call_sites.clear();
        let bag = crate::lower::lower(&unit.typed);
        assert!(
            bag.is_err(),
            "expected lowering to fail when method call sites are missing"
        );
    }
}

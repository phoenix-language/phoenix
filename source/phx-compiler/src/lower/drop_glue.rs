//! Compiler-inserted `Drop::drop` calls at scope exit.

use crate::ir::IrInst;
use crate::lower::ctx::LowerCtx;

/// Emits planned drops for scope depths `from_depth` down to `to_depth` (inclusive).
pub fn emit_scope_drops(ctx: &mut LowerCtx<'_>, from_depth: u32, to_depth: u32) {
    if from_depth < to_depth {
        return;
    }
    for depth in (to_depth..=from_depth).rev() {
        emit_drops_at_depth(ctx, depth);
    }
}

fn emit_drops_at_depth(ctx: &mut LowerCtx<'_>, depth: u32) {
    let mut events: Vec<_> = ctx
        .layout
        .drop_events
        .iter()
        .filter(|e| e.scope_depth == depth && !ctx.emitted_drop_slots.contains(&e.slot))
        .collect();
    events.sort_by_key(|e| e.slot.index());
    for event in events.into_iter().rev() {
        ctx.emitted_drop_slots.insert(event.slot);
        ctx.emit(
            event.span,
            IrInst::DropLocal {
                slot: event.slot,
                ty: event.ty,
                drop_fn: event.drop_fn,
                prim_kind: event.prim_kind,
            },
        );
    }
}

/// Returns the loop-body scope depth for the innermost active loop.
#[must_use]
pub fn loop_body_scope_depth(ctx: &LowerCtx<'_>) -> u32 {
    ctx.loop_body_scope_depths
        .last()
        .copied()
        .map_or(0, |d| d.saturating_add(1))
}

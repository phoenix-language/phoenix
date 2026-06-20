//! Drop glue: compiler-inserted [`IrInst::DropLocal`] at scope exit.
//!
//! Typeck records planned drops in [`FunctionLayout::drop_events`](crate::typeck::FunctionLayout)
//! keyed by binding scope depth. Lowering emits those drops when a scope closes, on `return`, on
//! loop `break`, and when a `for-in` iterator exhausts — always in reverse slot order so nested
//! bindings drop before outer ones.
//!
//! ## Invariants
//!
//! - **Once per slot:** [`LowerCtx::emitted_drop_slots`] prevents duplicate drops when multiple
//!   exit paths run for the same binding.
//! - **Depth range:** [`emit_scope_drops`] walks `(to_depth..=from_depth)` in reverse; a no-op when
//!   `from_depth < to_depth` (e.g. dropping only loop-body locals on `break`).
//! - **Loop body depth:** [`loop_body_scope_depth`] is `loop_scope + 1`, matching the scope opened
//!   for bindings introduced inside the loop body.

use crate::ir::IrInst;
use crate::lower::ctx::LowerCtx;

/// Emits planned drops for scope depths `from_depth` down to `to_depth` (inclusive).
///
/// Each depth emits [`IrInst::DropLocal`] for events in [`FunctionLayout::drop_events`] that have
/// not yet been recorded in [`LowerCtx::emitted_drop_slots`]. Used on scope exit, `return`, `break`,
/// and for-in exhaustion.
pub fn emit_scope_drops(ctx: &mut LowerCtx<'_>, from_depth: u32, to_depth: u32) {
    if from_depth < to_depth {
        return;
    }
    for depth in (to_depth..=from_depth).rev() {
        emit_drops_at_depth(ctx, depth);
    }
}

/// Emits all pending drop events at a single scope `depth`.
///
/// Events are sorted by slot index then emitted in reverse order so higher slot indices (typically
/// later bindings) drop first within the depth.
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

/// Scope depth of bindings that should drop on `break` from the innermost active loop.
///
/// Returns one greater than the scope depth recorded when the loop was entered, or `0` when no
/// loop is active. Paired with [`emit_scope_drops`] so `break` drops loop-body locals but not
/// enclosing function locals.
#[must_use]
pub fn loop_body_scope_depth(ctx: &LowerCtx<'_>) -> u32 {
    ctx.loop_body_scope_depths
        .last()
        .copied()
        .map_or(0, |d| d.saturating_add(1))
}

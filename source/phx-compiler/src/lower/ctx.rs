//! Shared lowering context: expression-type cursor and CFG builder.

use phx_diagnostics::Span;
use phx_syntax::Symbol;

use crate::ir::{IrBasicBlock, IrInst};
use crate::resolver::{DefId, ResolutionKey, ResolvedProgram};
use crate::typeck::{ExprId, FunctionLayout, LocalSlot, TypeId, TypedProgram};

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
        }
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

/// Maps a resolved local binding to its slot.
#[must_use]
pub fn slot_for_symbol(layout: &FunctionLayout, symbol: Symbol) -> Option<LocalSlot> {
    layout.binding(symbol).map(|b| b.slot)
}

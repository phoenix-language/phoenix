//! Lint pass for a type-checked Phoenix program.
//!
//! ## Pass role
//!
//! Runs after [`crate::typeck`] and before lowering. Consumes [`TypedProgram`] (resolved AST,
//! definition attribute metadata, and name resolutions) and emits non-fatal warnings in a
//! [`LintBag`]. Invalid `#[allow(...)]` attribute names are reported as resolve-style errors in
//! a [`DiagnosticBag`] instead of warnings.
//!
//! ## Lints
//!
//! | Kind | Trigger |
//! |------|---------|
//! | [`LintKind::Deprecated`] | Reference to a definition carrying `#[deprecated(...)]` |
//! | [`LintKind::MustUse`] | Expression statement or block tail whose value comes from a `#[must_use]` item |
//!
//! Std `Result` / `Option` discards are enforced in typeck
//! ([`phx_diagnostics::TypeCheckError::DiscardedStdResult`]); this pass only checks
//! attribute-driven `#[must_use]` on user definitions.
//!
//! ## `#[allow(...)]`
//!
//! Function and module-item attributes suppress lints lexically within the function body via a
//! stacked allow set ([`LintWalker::allow_stack`]). Names are parsed by [`crate::attrs::parse_allow_lint_kinds`].
//!
//! ## Entry point
//!
//! [`lint_program`] — called from [`crate::compile::lint_checked`].

use std::collections::HashSet;
use std::fmt::Write;

use phx_diagnostics::{DiagnosticBag, Lint, LintBag, LintKind, ResolveError, Span};
use phx_syntax::ast::decl::{Function, ImplMember, TopLevelDecl, TopLevelItem};
use phx_syntax::ast::expr::{Expr, IfCondition, PostfixOp};
use phx_syntax::ast::stmt::{Block, BlockItem, Stmt};
use phx_syntax::ast::{ExprNode, Node};
use phx_syntax::{Interner, Symbol};

use crate::attrs::parse_allow_lint_kinds;
use crate::resolver::{DefId, ResolutionKey};
use crate::typeck::TypedProgram;

/// Runs lint checks on a type-checked program.
///
/// Walks every function body in every loaded module, collecting deprecated-use and must-use
/// discard warnings. Does not mutate the AST.
///
/// # Errors
///
/// Returns [`DiagnosticBag`] when `#[allow(...)]` uses an unknown lint name on a module item or
/// function.
pub fn lint_program(typed: &TypedProgram) -> Result<LintBag, DiagnosticBag> {
    let resolved = &typed.resolved;
    let mut bag = DiagnosticBag::new();
    let mut lints = LintBag::new();
    for module in &resolved.modules {
        for (span, msg) in validate_allow_attrs(&module.program.items, &resolved.interner) {
            bag.push(
                module.id,
                ResolveError::InvalidCfg {
                    span,
                    message: format!("invalid `#[allow]`: {msg}"),
                },
            );
        }
        let mut walker = LintWalker {
            module: module.id,
            typed,
            interner: &resolved.interner,
            allow_stack: vec![HashSet::new()],
            lints: &mut lints,
        };
        for item in &module.program.items {
            walker.walk_top_level_item(&item.inner);
        }
    }
    if bag.has_errors() {
        return Err(bag);
    }
    Ok(lints)
}

/// Validates `#[allow(...)]` on module items and nested function/method attrs before the walk.
fn validate_allow_attrs(items: &[Node<TopLevelItem>], interner: &Interner) -> Vec<(Span, String)> {
    let mut errors = Vec::new();
    for item in items {
        if let Err(msg) = parse_allow_lint_kinds(interner, &item.inner.attrs)
            && let Some(attr) = item
                .inner
                .attrs
                .iter()
                .find(|a| interner.resolves_to(a.inner.name.symbol, "allow"))
        {
            errors.push((attr.span, msg));
        }
        match &item.inner.decl {
            TopLevelDecl::Function(f) => check_fn_allow(f, interner, &mut errors),
            TopLevelDecl::Impl { members, .. } => {
                for member in members {
                    if let ImplMember::Method(f) = member {
                        check_fn_allow(f, interner, &mut errors);
                    }
                }
            }
            _ => {}
        }
    }
    errors
}

/// Records invalid `#[allow(...)]` on a single function or impl method.
fn check_fn_allow(f: &Function, interner: &Interner, errors: &mut Vec<(Span, String)>) {
    if let Err(msg) = parse_allow_lint_kinds(interner, &f.attrs)
        && let Some(attr) = f
            .attrs
            .iter()
            .find(|a| interner.resolves_to(a.inner.name.symbol, "allow"))
    {
        errors.push((attr.span, msg));
    }
}

/// AST visitor that emits lints while honoring nested `#[allow(...)]` scopes.
struct LintWalker<'a> {
    /// Module id for resolution keys and lint routing.
    module: u32,
    /// Type-checked program (AST, resolutions, def attrs).
    typed: &'a TypedProgram,
    /// Symbol interner for lint messages.
    interner: &'a Interner,
    /// Stack of allowed lint kinds; inner scopes inherit outer allows.
    allow_stack: Vec<HashSet<LintKind>>,
    /// Collected warnings for the current compilation.
    lints: &'a mut LintBag,
}

impl LintWalker<'_> {
    fn walk_top_level_item(&mut self, item: &TopLevelItem) {
        match &item.decl {
            TopLevelDecl::Function(f) => self.walk_function(f, &item.attrs),
            TopLevelDecl::Impl { members, .. } => {
                for member in members {
                    if let ImplMember::Method(f) = member {
                        self.walk_function(f, &[]);
                    }
                }
            }
            _ => {}
        }
    }

    fn walk_function(&mut self, f: &Function, outer_attrs: &[Node<phx_syntax::ast::Attribute>]) {
        let mut allowed = self.allow_stack.last().cloned().unwrap_or_default();
        for kind in parse_allow_lint_kinds(self.interner, outer_attrs).unwrap_or_default() {
            allowed.insert(kind);
        }
        for kind in parse_allow_lint_kinds(self.interner, &f.attrs).unwrap_or_default() {
            allowed.insert(kind);
        }
        self.allow_stack.push(allowed);
        self.walk_block(&f.body.inner);
        self.allow_stack.pop();
    }

    fn walk_block(&mut self, block: &Block) {
        for item in &block.items {
            match item {
                BlockItem::Stmt(stmt) => self.walk_stmt(&stmt.inner),
                BlockItem::Expr(expr) => {
                    self.check_discard(expr);
                    self.walk_expr(expr);
                }
                BlockItem::Import(_) => {}
            }
        }
    }

    fn walk_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Const { init, .. } | Stmt::Var { init, .. } => self.walk_expr(init),
            Stmt::Assign { expr } => self.walk_expr(expr),
            Stmt::Return(Some(e)) => self.walk_expr(e),
            Stmt::Expr(expr) => {
                self.check_discard(expr);
                self.walk_expr(expr);
            }
            Stmt::While { cond, body } => {
                self.walk_expr(cond);
                self.walk_block(&body.inner);
            }
            Stmt::ForIn { iter, body, .. } => {
                self.walk_expr(iter);
                self.walk_block(&body.inner);
            }
            Stmt::Loop(body) | Stmt::Unsafe(body) => self.walk_block(&body.inner),
            Stmt::Break { value: Some(v), .. } => self.walk_expr(v),
            _ => {}
        }
    }

    fn walk_expr(&mut self, expr: &ExprNode) {
        self.check_deprecated_use(expr);
        match &expr.inner {
            Expr::Unary { operand, .. } => self.walk_expr(operand),
            Expr::Binary { left, right, .. } => {
                self.walk_expr(left);
                self.walk_expr(right);
            }
            Expr::Assign { target, value, .. } => {
                self.walk_expr(target);
                self.walk_expr(value);
            }
            Expr::Cast { expr: inner, .. } => self.walk_expr(inner),
            Expr::Postfix { base, ops } => {
                self.walk_expr(base);
                for op in ops {
                    match op {
                        PostfixOp::Call { args, .. } | PostfixOp::Method { args, .. } => {
                            for a in args {
                                self.walk_expr(a);
                            }
                        }
                        PostfixOp::Index(idx) => self.walk_expr(idx),
                        _ => {}
                    }
                }
            }
            Expr::If {
                condition,
                then_block,
                else_ifs,
                else_block,
            } => {
                self.walk_if_condition(condition.as_ref());
                self.walk_block(&then_block.inner);
                for (c, b) in else_ifs {
                    self.walk_if_condition(c);
                    self.walk_block(&b.inner);
                }
                if let Some(b) = else_block {
                    self.walk_block(&b.inner);
                }
            }
            Expr::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee);
                for arm in arms {
                    self.walk_expr(&arm.body);
                }
            }
            Expr::Block(b) | Expr::Unsafe(b) => self.walk_block(&b.inner),
            Expr::StructLit { fields, .. } => {
                for field in fields {
                    if let phx_syntax::ast::expr::StructFieldInit::Field { value, .. } = field {
                        self.walk_expr(value);
                    }
                }
            }
            Expr::Tuple(items) | Expr::Array(items) => {
                for e in items {
                    self.walk_expr(e);
                }
            }
            Expr::Lambda { body, .. } => match body {
                phx_syntax::ast::expr::LambdaBody::Expr(e) => self.walk_expr(e),
                phx_syntax::ast::expr::LambdaBody::Block(b) => self.walk_block(&b.inner),
            },
            Expr::Range { start, end, .. } => {
                self.walk_expr(start);
                self.walk_expr(end);
            }
            Expr::RuntimeDirective { args, .. } => {
                for a in args {
                    self.walk_expr(a);
                }
            }
            _ => {}
        }
    }

    fn walk_if_condition(&mut self, condition: &IfCondition) {
        match condition {
            IfCondition::Bool(cond) => self.walk_expr(cond),
            IfCondition::Pattern { scrutinee, .. } => self.walk_expr(scrutinee),
        }
    }

    /// Emits [`LintKind::Deprecated`] when an identifier or path resolves to a deprecated def.
    fn check_deprecated_use(&mut self, expr: &ExprNode) {
        if self.is_allowed(LintKind::Deprecated) {
            return;
        }
        let def_id = match &expr.inner {
            Expr::Ident(ident) => self.resolve_node(ident.id),
            Expr::Path(path) => self.resolve_path(path),
            _ => return,
        };
        let Some(def_id) = def_id else {
            return;
        };
        let Some(attrs) = self.typed.resolved.def_attrs.get(&def_id) else {
            return;
        };
        let Some(meta) = &attrs.deprecated else {
            return;
        };
        let name = self
            .typed
            .resolved
            .defs
            .get(def_id.index() as usize)
            .map_or("item", |d| self.interner.resolve(d.name).unwrap_or("<?>"));
        let mut message = format!("use of deprecated item `{name}`");
        if let Some(since) = &meta.since {
            let _ = write!(message, " (since {since})");
        }
        let mut notes = Vec::new();
        if let Some(note) = &meta.note {
            notes.push(note.clone());
        }
        if let Some(suggestion) = &meta.suggestion {
            notes.push(format!("use `{suggestion}` instead"));
        }
        self.lints.push(
            self.module,
            Lint {
                kind: LintKind::Deprecated,
                span: expr.span,
                message,
                notes,
            },
        );
    }

    /// Emits [`LintKind::MustUse`] for expression statements and block tails that drop a must-use value.
    fn check_discard(&mut self, expr: &ExprNode) {
        if self.is_allowed(LintKind::MustUse) {
            return;
        }
        let Some(reason) = self.discard_must_use_reason(expr) else {
            return;
        };
        self.lints.push(
            self.module,
            Lint {
                kind: LintKind::MustUse,
                span: expr.span,
                message: reason,
                notes: Vec::new(),
            },
        );
    }

    /// Returns a lint message when `expr` is used as a discarded value and carries `#[must_use]`.
    fn discard_must_use_reason(&self, expr: &ExprNode) -> Option<String> {
        if self.expr_attr_must_use(expr) {
            return Some("unused result of `#[must_use]` item".to_string());
        }
        None
    }

    /// Whether `expr` evaluates to a value from a definition marked `#[must_use]`.
    fn expr_attr_must_use(&self, expr: &ExprNode) -> bool {
        match &expr.inner {
            Expr::Postfix { base, ops } if matches!(ops.last(), Some(PostfixOp::Call { .. })) => {
                self.callee_def_id(base)
                    .is_some_and(|id| self.def_must_use(id))
            }
            Expr::StructLit { name, .. } => self.type_name_must_use(name.symbol),
            Expr::Ident(ident) => self
                .resolve_node(ident.id)
                .is_some_and(|id| self.def_must_use(id)),
            Expr::Path(path) => self
                .resolve_path(path)
                .is_some_and(|id| self.def_must_use(id)),
            _ => false,
        }
    }

    fn callee_def_id(&self, expr: &ExprNode) -> Option<DefId> {
        match &expr.inner {
            Expr::Ident(ident) => self.resolve_node(ident.id),
            Expr::Path(path) => self.resolve_path(path),
            Expr::Postfix { base, .. } => self.callee_def_id(base),
            _ => None,
        }
    }

    fn def_must_use(&self, def_id: DefId) -> bool {
        self.typed
            .resolved
            .def_attrs
            .get(&def_id)
            .is_some_and(|a| a.must_use)
    }

    fn type_name_must_use(&self, sym: Symbol) -> bool {
        self.typed
            .resolved
            .defs
            .iter()
            .enumerate()
            .find_map(|(i, d)| {
                if d.name == sym
                    && matches!(
                        d.kind,
                        crate::resolver::DefKind::Struct | crate::resolver::DefKind::Enum
                    )
                {
                    self.typed
                        .resolved
                        .def_attrs
                        .get(&DefId::from_raw(u32::try_from(i).unwrap_or(u32::MAX)))
                        .filter(|a| a.must_use)
                        .map(|_| true)
                } else {
                    None
                }
            })
            .unwrap_or(false)
    }

    fn resolve_node(&self, node_id: phx_syntax::AstNodeId) -> Option<DefId> {
        self.typed
            .resolved
            .resolutions
            .get(&ResolutionKey {
                module: self.module,
                node_id,
            })
            .copied()
    }

    fn resolve_path(&self, path: &phx_syntax::ast::ident::Path) -> Option<DefId> {
        use phx_syntax::ast::ident::PathSegment;
        let seg = path.segments.last()?;
        let node_id = match seg {
            PathSegment::Ident(i) => i.id,
            PathSegment::Type(t) => t.name.id,
        };
        self.resolve_node(node_id)
    }

    /// Whether the innermost allow scope suppresses `kind`.
    fn is_allowed(&self, kind: LintKind) -> bool {
        self.allow_stack
            .last()
            .is_some_and(|set| set.contains(&kind))
    }
}

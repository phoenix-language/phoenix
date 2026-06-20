//! Collect all `#import` directives in a parsed program.
//!
//! Phoenix allows `#import` at file scope (before top-level items) and inside blocks (function
//! bodies, nested scopes, control-flow arms). Resolver and module passes need every directive
//! regardless of nesting depth — this module walks the untyped AST and returns them in
//! discovery order.
//!
//! ## What is collected
//!
//! | Location | AST path |
//! |----------|----------|
//! | File header | [`Program::imports`](crate::ast::Program::imports) |
//! | Block scope | [`BlockItem::Import`](crate::ast::stmt::BlockItem::Import) inside functions, `if` arms, loops, etc. |
//!
//! Directives nested inside expressions (for example within a closure body) are included. Type
//! and item declarations without executable bodies are skipped.
//!
//! ## Public API
//!
//! - [`all_imports`] — single entry point for the full list.
//!
//! ## Pipeline position
//!
//! Called by resolver/module logic after [`crate::parse`]. Does not validate paths or resolve
//! symbols — it only enumerates syntax nodes.

use crate::ast::decl::{ImplMember, ImportDirective, Program, TopLevelDecl, TopLevelItem};
use crate::ast::expr::{Expr, IfCondition, LambdaBody};
use crate::ast::pat::MatchArm;
use crate::ast::stmt::{Block, BlockItem, Stmt};
use crate::ast::{BlockNode, Node};

/// Returns every `#import` directive in `program`, in depth-first discovery order.
///
/// File-level imports ([`Program::imports`](crate::ast::Program::imports)) appear first, in
/// source order, followed by block-scoped imports encountered while walking top-level items and
/// their nested expressions, statements, and control-flow bodies.
///
/// Each returned node carries the directive's [`Span`](crate::Span) for diagnostics.
///
/// # Panics
///
/// Never panics — read-only traversal of a parsed AST.
///
/// # Examples
///
/// File-level import:
///
/// ```
/// use phx_syntax::{all_imports, parse};
///
/// let src = "#import std::io;\nmain :: () => { };";
/// let file = parse(src);
/// assert!(!file.has_errors());
/// assert_eq!(all_imports(&file.value.program).len(), 1);
/// ```
///
/// Block-scoped import inside a function body:
///
/// ```
/// use phx_syntax::{all_imports, parse};
///
/// let src = r"
/// main :: () => {
///   #import util::math::add;
///   const _ = add(1, 2);
/// };
/// ";
/// let file = parse(src);
/// assert!(!file.has_errors());
/// assert_eq!(all_imports(&file.value.program).len(), 1);
/// ```
#[must_use]
pub fn all_imports(program: &Program) -> Vec<&Node<ImportDirective>> {
    let mut out = Vec::new();
    for imp in &program.imports {
        out.push(imp);
    }
    for item in &program.items {
        walk_top_level_item(&item.inner, &mut out);
    }
    out
}

fn walk_top_level_item<'a>(item: &'a TopLevelItem, out: &mut Vec<&'a Node<ImportDirective>>) {
    match &item.decl {
        TopLevelDecl::Function(f) => walk_block(&f.body.inner, out),
        TopLevelDecl::Impl { members, .. } => {
            for m in members {
                if let ImplMember::Method(f) = m {
                    walk_block(&f.body.inner, out);
                }
            }
        }
        TopLevelDecl::Const { init, .. } | TopLevelDecl::Var { init, .. } => {
            walk_expr(&init.inner, out);
        }
        _ => {}
    }
}

fn walk_block<'a>(block: &'a Block, out: &mut Vec<&'a Node<ImportDirective>>) {
    for item in &block.items {
        match item {
            BlockItem::Import(imp) => out.push(imp),
            BlockItem::Stmt(stmt) => walk_stmt(&stmt.inner, out),
            BlockItem::Expr(expr) => walk_expr(&expr.inner, out),
        }
    }
}

fn walk_block_node<'a>(block: &'a BlockNode, out: &mut Vec<&'a Node<ImportDirective>>) {
    walk_block(&block.inner, out);
}

fn walk_stmt<'a>(stmt: &'a Stmt, out: &mut Vec<&'a Node<ImportDirective>>) {
    match stmt {
        Stmt::Const { init, .. } | Stmt::Var { init, .. } => walk_expr(&init.inner, out),
        Stmt::Expr(expr) | Stmt::Assign { expr } => walk_expr(&expr.inner, out),
        Stmt::Return(expr) => {
            if let Some(e) = expr {
                walk_expr(&e.inner, out);
            }
        }
        Stmt::Break { value, .. } => {
            if let Some(e) = value {
                walk_expr(&e.inner, out);
            }
        }
        Stmt::Continue { .. } => {}
        Stmt::While { cond, body } => {
            walk_expr(&cond.inner, out);
            walk_block_node(body, out);
        }
        Stmt::ForIn { iter, body, .. } => {
            walk_expr(&iter.inner, out);
            walk_block_node(body, out);
        }
        Stmt::Loop(body) | Stmt::Unsafe(body) => walk_block_node(body, out),
    }
}

fn walk_if_condition<'a>(condition: &'a IfCondition, out: &mut Vec<&'a Node<ImportDirective>>) {
    match condition {
        IfCondition::Bool(cond) => walk_expr(&cond.inner, out),
        IfCondition::Pattern { scrutinee, .. } => walk_expr(&scrutinee.inner, out),
    }
}

fn walk_expr<'a>(expr: &'a Expr, out: &mut Vec<&'a Node<ImportDirective>>) {
    match expr {
        Expr::Unary { operand, .. } => walk_expr(&operand.inner, out),
        Expr::Binary { left, right, .. } => {
            walk_expr(&left.inner, out);
            walk_expr(&right.inner, out);
        }
        Expr::Assign { target, value, .. } => {
            walk_expr(&target.inner, out);
            walk_expr(&value.inner, out);
        }
        Expr::Cast { expr, .. } => walk_expr(&expr.inner, out),
        Expr::Postfix { base, ops } => {
            walk_expr(&base.inner, out);
            for op in ops {
                if let crate::ast::expr::PostfixOp::Call { args, .. }
                | crate::ast::expr::PostfixOp::Method { args, .. } = op
                {
                    for arg in args {
                        walk_expr(&arg.inner, out);
                    }
                } else if let crate::ast::expr::PostfixOp::Index(idx) = op {
                    walk_expr(&idx.inner, out);
                }
            }
        }
        Expr::If {
            condition,
            then_block,
            else_ifs,
            else_block,
        } => {
            walk_if_condition(condition.as_ref(), out);
            walk_block_node(then_block, out);
            for (c, b) in else_ifs {
                walk_if_condition(c, out);
                walk_block_node(b, out);
            }
            if let Some(b) = else_block {
                walk_block_node(b, out);
            }
        }
        Expr::Match { scrutinee, arms } => {
            walk_expr(&scrutinee.inner, out);
            for arm in arms {
                walk_match_arm(arm, out);
            }
        }
        Expr::Block(block) | Expr::Unsafe(block) => walk_block_node(block, out),
        Expr::StructLit { fields, .. } => {
            for field in fields {
                if let crate::ast::expr::StructFieldInit::Field { value, .. } = field {
                    walk_expr(&value.inner, out);
                } else if let crate::ast::expr::StructFieldInit::Spread(e) = field {
                    walk_expr(&e.inner, out);
                }
            }
        }
        Expr::Range { start, end, .. } => {
            walk_expr(&start.inner, out);
            walk_expr(&end.inner, out);
        }
        Expr::Lambda { body, .. } => walk_lambda_body(body, out),
        Expr::RuntimeDirective { args, .. } => {
            for arg in args {
                walk_expr(&arg.inner, out);
            }
        }
        Expr::Tuple(elems) | Expr::Array(elems) => {
            for e in elems {
                walk_expr(&e.inner, out);
            }
        }
        Expr::Literal(_) | Expr::Ident(_) | Expr::Path(_) => {}
    }
}

fn walk_lambda_body<'a>(body: &'a LambdaBody, out: &mut Vec<&'a Node<ImportDirective>>) {
    match body {
        LambdaBody::Expr(e) => walk_expr(&e.inner, out),
        LambdaBody::Block(b) => walk_block_node(b, out),
    }
}

fn walk_match_arm<'a>(arm: &'a MatchArm, out: &mut Vec<&'a Node<ImportDirective>>) {
    if let Some(guard) = &arm.guard {
        walk_expr(&guard.inner, out);
    }
    walk_expr(&arm.body.inner, out);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::parse;

    #[test]
    fn all_imports_includes_file_and_block() {
        let src = r"
main :: () => {
  #import util::math::add;
  const _ = add(1, 2);
};
";
        let file = parse(src);
        assert!(!file.has_errors(), "parse: {:?}", file.errors);
        let imports = all_imports(&file.value.program);
        assert_eq!(imports.len(), 1);
    }

    #[test]
    fn all_imports_nested_block() {
        let src = r"
main :: () => {
  {
    #import util::math::{ add, mul };
    const _ = add(1, mul(2, 3));
  };
};
";
        let file = parse(src);
        assert!(!file.has_errors(), "parse: {:?}", file.errors);
        assert_eq!(all_imports(&file.value.program).len(), 1);
    }

    #[test]
    fn all_imports_if_arm() {
        let src = r"
main :: () => {
  if true {
    #import util::math::add;
    const _ = add(1, 2);
  } else {
    const _ = 0;
  };
};
";
        let file = parse(src);
        assert!(!file.has_errors(), "parse: {:?}", file.errors);
        assert_eq!(all_imports(&file.value.program).len(), 1);
    }
}

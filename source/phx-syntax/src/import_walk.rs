//! Collect all `#import` directives in a program (file scope and block scope).

use crate::ast::decl::{ImplMember, ImportDirective, Program, TopLevelDecl, TopLevelItem};
use crate::ast::expr::{Expr, LambdaBody};
use crate::ast::pat::MatchArm;
use crate::ast::stmt::{Block, BlockItem, Stmt};
use crate::ast::{BlockNode, Node};

/// Returns every `#import` in `program`, including block-scoped directives.
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
            BlockItem::Stmt(stmt) => walk_stmt(stmt, out),
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
        Stmt::Given {
            scrutinee, body, ..
        } => {
            walk_expr(&scrutinee.inner, out);
            walk_block_node(body, out);
        }
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
            cond,
            then_block,
            else_ifs,
            else_block,
        } => {
            walk_expr(&cond.inner, out);
            walk_block_node(then_block, out);
            for (e, b) in else_ifs {
                walk_expr(&e.inner, out);
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
        let file = parse(src).expect("parse");
        let imports = all_imports(&file.program);
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
        let file = parse(src).expect("parse");
        assert_eq!(all_imports(&file.program).len(), 1);
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
        let file = parse(src).expect("parse");
        assert_eq!(all_imports(&file.program).len(), 1);
    }
}

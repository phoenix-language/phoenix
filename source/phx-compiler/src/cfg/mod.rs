//! Conditional compilation (`#[cfg(...)]`) stripping before name resolution.
//!
//! Evaluates compile-time configuration predicates on module items and impl members, then
//! removes AST nodes whose `#[cfg(...)]` attributes are false. Inactive code never reaches
//! the resolver, type checker, or codegen.
//!
//! ## Supported predicates
//!
//! | Form | Meaning |
//! |------|---------|
//! | `target_os = "…"` | Matches [`CompileCfg::target_os`] |
//! | `target_arch = "…"` | Matches [`CompileCfg::target_arch`] |
//! | `debug_assertions` | True when [`CompileCfg::debug_assertions`] is set |
//! | `not(...)` | Negates a single nested predicate |
//!
//! Multiple arguments on one `#[cfg(...)]` are AND-ed. Items with no `#[cfg]` are always kept.
//! Associated types in impl blocks are never cfg-stripped.
//!
//! ## Pipeline position
//!
//! [`strip_cfg`] runs immediately after parse in [`crate::modules::loader`] and
//! [`crate::compile::compile_source`], before [`crate::derive::expand_derives`] and resolution.
//! [`CompileCfg::host`] supplies the default configuration from the build driver's target triple.

use phx_diagnostics::Span;
use phx_syntax::ast::Node;
use phx_syntax::ast::attr::{AttrArg, AttrValue, Attribute};
use phx_syntax::ast::decl::{ImplMember, TopLevelDecl, TopLevelItem};
use phx_syntax::{Interner, Program};

/// Host compile-time configuration for `#[cfg(...)]` evaluation.
///
/// Passed to [`strip_cfg`]. Tests and embedders may construct custom values to simulate
/// cross-target builds without recompiling the compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileCfg {
    /// `target_os` string (e.g. `linux`, `macos`, `windows`).
    pub target_os: String,
    /// `target_arch` string (e.g. `x86_64`, `aarch64`).
    pub target_arch: String,
    /// `true` when building with debug assertions enabled.
    pub debug_assertions: bool,
}

impl CompileCfg {
    /// Builds configuration from the host triple (build driver default).
    ///
    /// Uses [`std::env::consts::OS`] and [`std::env::consts::ARCH`] for target fields and
    /// [`cfg!(debug_assertions)`] for the debug flag.
    #[must_use]
    pub fn host() -> Self {
        Self {
            target_os: std::env::consts::OS.to_string(),
            target_arch: std::env::consts::ARCH.to_string(),
            debug_assertions: cfg!(debug_assertions),
        }
    }
}

/// Failure while evaluating or stripping `#[cfg(...)]`.
///
/// Surfaced as [`phx_diagnostics::ResolveError::InvalidCfg`] in the module loader and
/// single-file compile path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfgError {
    /// Related source span.
    pub span: Span,
    /// Human-readable message.
    pub message: String,
}

/// Removes items and impl members whose `#[cfg(...)]` predicates are false.
///
/// Mutates `program.items` in place: inactive top-level declarations are dropped, and
/// inactive methods inside `impl` blocks are removed. Function declarations combine item-level
/// and function-level `#[cfg]` with logical AND.
///
/// # Errors
///
/// Returns [`CfgError`] when a cfg predicate uses an unknown key, malformed argument, or
/// invalid `not(...)` arity.
pub fn strip_cfg(
    program: &mut Program,
    compile_cfg: &CompileCfg,
    interner: &Interner,
) -> Result<(), CfgError> {
    let taken = std::mem::take(&mut program.items);
    let mut kept = Vec::with_capacity(taken.len());
    for mut item in taken {
        if !cfg_item_active(&item.inner, compile_cfg, interner)? {
            continue;
        }
        if let TopLevelDecl::Impl { members, .. } = &mut item.inner.decl {
            let old = std::mem::take(members);
            let mut next = Vec::with_capacity(old.len());
            for member in old {
                let keep = match member {
                    ImplMember::Method(ref f) => cfg_attrs_active(&f.attrs, compile_cfg, interner)?,
                    ImplMember::AssociatedType { .. } => true,
                };
                if keep {
                    next.push(member);
                }
            }
            *members = next;
        }
        kept.push(item);
    }
    program.items = kept;
    Ok(())
}

fn cfg_item_active(
    item: &TopLevelItem,
    compile_cfg: &CompileCfg,
    interner: &Interner,
) -> Result<bool, CfgError> {
    let mut active = cfg_attrs_active(&item.attrs, compile_cfg, interner)?;
    if let TopLevelDecl::Function(f) = &item.decl {
        active &= cfg_attrs_active(&f.attrs, compile_cfg, interner)?;
    }
    Ok(active)
}

fn cfg_attrs_active(
    attrs: &[Node<Attribute>],
    compile_cfg: &CompileCfg,
    interner: &Interner,
) -> Result<bool, CfgError> {
    let mut saw_cfg = false;
    let mut active = true;
    for attr in attrs {
        if !interner.resolves_to(attr.inner.name.symbol, "cfg") {
            continue;
        }
        saw_cfg = true;
        if !eval_cfg_args(&attr.inner.args, attr.span, compile_cfg, interner)? {
            active = false;
        }
    }
    if saw_cfg { Ok(active) } else { Ok(true) }
}

fn eval_cfg_args(
    args: &[AttrArg],
    span: Span,
    compile_cfg: &CompileCfg,
    interner: &Interner,
) -> Result<bool, CfgError> {
    if args.is_empty() {
        return Err(CfgError {
            span,
            message: "`#[cfg]` requires at least one predicate".to_string(),
        });
    }
    for arg in args {
        if !eval_cfg_arg(arg, span, compile_cfg, interner)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn eval_cfg_arg(
    arg: &AttrArg,
    span: Span,
    compile_cfg: &CompileCfg,
    interner: &Interner,
) -> Result<bool, CfgError> {
    match arg {
        AttrArg::Named { name, value } => match interner.resolve(name.symbol) {
            Some("target_os") => {
                let AttrValue::Str(expected) = value else {
                    return Err(invalid_value(span, "target_os"));
                };
                Ok(compile_cfg.target_os == *expected)
            }
            Some("target_arch") => {
                let AttrValue::Str(expected) = value else {
                    return Err(invalid_value(span, "target_arch"));
                };
                Ok(compile_cfg.target_arch == *expected)
            }
            Some(other) => Err(unknown_key(span, other)),
            None => Err(unknown_key(span, "<?>")),
        },
        AttrArg::Flag(ident) => match interner.resolve(ident.symbol) {
            Some("debug_assertions") => Ok(compile_cfg.debug_assertions),
            Some(other) => Err(unknown_key(span, other)),
            None => Err(unknown_key(span, "<?>")),
        },
        AttrArg::Nested { name, args } => match interner.resolve(name.symbol) {
            Some("not") => {
                if args.len() != 1 {
                    return Err(CfgError {
                        span,
                        message: "`not(...)` requires exactly one predicate".to_string(),
                    });
                }
                Ok(!eval_cfg_arg(&args[0], span, compile_cfg, interner)?)
            }
            Some(other) => Err(unknown_key(span, other)),
            None => Err(unknown_key(span, "<?>")),
        },
        AttrArg::TypeName(_) => Err(CfgError {
            span,
            message: "unexpected type name in `#[cfg]` predicate".to_string(),
        }),
    }
}

fn unknown_key(span: Span, key: &str) -> CfgError {
    CfgError {
        span,
        message: format!("unknown `#[cfg]` predicate `{key}`"),
    }
}

fn invalid_value(span: Span, key: &str) -> CfgError {
    CfgError {
        span,
        message: format!("`#[cfg]` predicate `{key}` requires a string literal"),
    }
}

//! Declaration AST.
//!
//! Top-level [`Program`] items, functions, user types, traits, impls, and `#import` metadata.

use crate::ast::Node;
use crate::ast::attr::Attribute;
use crate::ast::expr::ExprNode;
use crate::ast::ident::{Ident, Path, TypeName};
use crate::ast::stmt::BlockNode;
use crate::ast::types::{GenericParam, Type};

/// Function parameter.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Param {
    /// `self` / `mut self` / `self: T` / `mut self: T`.
    Receiver {
        /// `mut` present.
        mut_: bool,
        /// Optional explicit type.
        ty: Option<Node<Type>>,
    },
    /// `name: Type`.
    Named {
        /// Parameter name.
        name: Ident,
        /// Parameter type.
        ty: Node<Type>,
    },
}

/// `#derive(Trait, …)` — parsed; codegen deferred.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeriveDirective {
    /// Trait names to derive.
    pub traits: Vec<TypeName>,
}

/// Compile-time function directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FnDirective {
    /// `#inline`
    Inline,
    /// `#cold`
    Cold,
    /// `#hot`
    Hot,
}

/// `#import` directive.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportDirective {
    /// Module path.
    pub path: Path,
    /// Optional `::{ a, b, * }` list.
    pub items: Option<ImportItems>,
}

/// Items in a braced import list.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ImportItem {
    /// Single identifier import.
    Ident(Ident),
    /// `*` glob import.
    Glob,
}

/// Braced import item list.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportItems {
    /// Imported symbols.
    pub items: Vec<ImportItem>,
}

/// Struct body shape.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum StructBody {
    /// `struct { fields }`.
    Fields(Vec<StructField>),
    /// Tuple struct `struct(T, U)`.
    Tuple(Vec<Node<Type>>),
    /// Unit struct with no body tokens.
    Unit,
}

/// Field in a struct definition.
#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    /// Field name.
    pub name: Ident,
    /// Field type.
    pub ty: Node<Type>,
}

/// Enum variant shape.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Variant {
    /// Unit variant `Eof`.
    Unit,
    /// Struct variant `Number { … }`.
    Struct(Vec<StructField>),
    /// Tuple variant `Bytes(…)`.
    Tuple(Vec<Node<Type>>),
}

/// Enum variant definition.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    /// Variant name.
    pub name: TypeName,
    /// Variant shape.
    pub kind: Variant,
}

/// Trait member.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TraitItem {
    /// `type Item;`
    AssociatedType(Ident),
    /// Method signature or definition.
    Method(FunctionSig),
}

/// Function signature without body.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionSig {
    /// Name.
    pub name: Ident,
    /// `unsafe` on the method (only when the enclosing trait is not `unsafe trait`).
    pub unsafe_: bool,
    /// Generic parameters.
    pub generics: Option<Vec<GenericParam>>,
    /// Parameters.
    pub params: Vec<Param>,
    /// Return type.
    pub ret: Option<Node<Type>>,
    /// Optional body when defined in trait.
    pub body: Option<BlockNode>,
}

/// Function definition with body.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    /// Bracket attributes (`#[...]`).
    pub attrs: Vec<Node<Attribute>>,
    /// `#derive` attributes (no codegen in MVP).
    pub derives: Vec<DeriveDirective>,
    /// Directives (`#inline`, …).
    pub directives: Vec<FnDirective>,
    /// `unsafe` on the function.
    pub unsafe_: bool,
    /// Name.
    pub name: Ident,
    /// Generic parameters.
    pub generics: Option<Vec<GenericParam>>,
    /// Parameters.
    pub params: Vec<Param>,
    /// Return type.
    pub ret: Option<Node<Type>>,
    /// Body block.
    pub body: BlockNode,
}

/// Trait impl member (method or associated type assignment).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ImplMember {
    /// `type Item = T;`
    AssociatedType {
        /// Associated type name.
        name: TypeName,
        /// Concrete type assigned on this impl.
        ty: Node<Type>,
    },
    /// Method definition.
    Method(Function),
}

/// Top-level declaration payload.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TopLevelDecl {
    /// `Name :: struct …`
    Struct {
        /// Type name.
        name: TypeName,
        /// `#derive` attributes.
        derives: Vec<DeriveDirective>,
        /// Generic parameters.
        generics: Option<Vec<GenericParam>>,
        /// Struct body.
        body: StructBody,
    },
    /// `Name :: enum …`
    Enum {
        /// Type name.
        name: TypeName,
        /// `#derive` attributes.
        derives: Vec<DeriveDirective>,
        /// Generic parameters.
        generics: Option<Vec<GenericParam>>,
        /// Variants.
        variants: Vec<EnumVariant>,
    },
    /// `type Name = T`
    TypeAlias {
        /// Alias name.
        name: TypeName,
        /// Generic parameters.
        generics: Option<Vec<GenericParam>>,
        /// Aliased type.
        ty: Node<Type>,
    },
    /// `Name :: trait { … }`
    Trait {
        /// Trait name.
        name: TypeName,
        /// `#derive` attributes.
        derives: Vec<DeriveDirective>,
        /// `unsafe trait` — all methods are effectively `unsafe fn`.
        unsafe_: bool,
        /// Generic parameters.
        generics: Option<Vec<GenericParam>>,
        /// Trait items.
        items: Vec<TraitItem>,
    },
    /// `Type :: impl [:: Trait] { … }`
    Impl {
        /// Implementing type.
        type_name: TypeName,
        /// Generic parameters on impl.
        generics: Option<Vec<GenericParam>>,
        /// `unsafe impl` — required when implementing an `unsafe trait`.
        unsafe_: bool,
        /// Optional trait (`PartialEq`, `From<Source>`, …).
        trait_: Option<Node<Type>>,
        /// Impl members.
        members: Vec<ImplMember>,
    },
    /// Function at top level.
    Function(Function),
    /// Top-level `const`.
    Const {
        /// Name.
        name: Ident,
        /// Optional type.
        ty: Option<Node<Type>>,
        /// Initializer.
        init: ExprNode,
    },
    /// Top-level `var`.
    Var {
        /// Name.
        name: Ident,
        /// Type.
        ty: Node<Type>,
        /// Initializer.
        init: ExprNode,
    },
    /// `mod name` — register a child module file.
    Mod {
        /// Module stem (`from_io` → `from_io.phx`).
        name: Ident,
    },
    /// `pub reexport :: path` — re-export a symbol at this module path.
    Reexport {
        /// Target path after leading `::` (`Item` or `child::Item`).
        path: Path,
    },
    /// `extern "C" { … }` — foreign symbol block.
    ExternBlock {
        /// ABI string literal (e.g. `"C"`).
        abi: String,
        /// Imported foreign signatures.
        items: Vec<FunctionSig>,
    },
    /// `extern "C" name :: (…) => T` — single foreign symbol.
    ExternItem {
        /// ABI string literal.
        abi: String,
        /// Foreign signature.
        sig: FunctionSig,
    },
}

/// Top-level item with optional `pub`.
#[derive(Debug, Clone, PartialEq)]
pub struct TopLevelItem {
    /// Bracket attributes (`#[...]`).
    pub attrs: Vec<Node<Attribute>>,
    /// `pub` modifier present.
    pub pub_: bool,
    /// Declaration.
    pub decl: TopLevelDecl,
}

/// Parsed program (one file / module).
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// File-level imports.
    pub imports: Vec<Node<ImportDirective>>,
    /// Top-level items.
    pub items: Vec<Node<TopLevelItem>>,
}

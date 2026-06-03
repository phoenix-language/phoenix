# Grammar overview

Surface syntax for Phoenix MVP. This is the human-oriented companion to [grammar.ebnf](grammar.ebnf).

The MVP grammar is compiler-first: enough to build lexer/parser/type-checker/bytecode lowering without relying on post-MVP runtime features.

---

## MVP entrypoint

Every executable must include:

```phoenix
main :: () => { /* ... */ }
```

- `main` does not need `pub`.
- File name is irrelevant.
- Return type may be omitted and defaults to `()`.

---

## Primitive data model (MVP)

Phoenix MVP does **not** include a primitive `string` type.

- Signed integers: `s8`, `s16`, `s32`, `s64`, `s128`
- Unsigned integers: `u8`, `u16`, `u32`, `u64`, `u128`
- Floats: `f32`, `f64`
- `bool`
- Raw pointers: `*T`, `*mut T`
- Borrow types: `&T`, `&mut T`
- Fixed arrays: `[T; N]`
- Slices/views: `[T]`
- Unit: `()`
- Built-in generics: `Option<T>`, `Result<T, E>`

Literal defaults:

```phoenix
const a = 42;      // s32
const b = 42u;     // u32
const c = 3.14;    // f32
```

---

## Comments

```phoenix
// line comment

///
 block comment
///
```

---

## Identifiers and keywords

- Values/functions: `snake_case`
- Types/variants/traits: `PascalCase`

Core keywords include:
`const`, `var`, `if`, `else`, `match`, `given`, `while`, `for`, `loop`, `break`, `continue`, `return`, `struct`, `enum`, `type`, `pub`, `trait`, `impl`, `as`, `true`, `false`, `self`

Declarations use `Name :: kind` — for example `Point :: struct`, `PartialEq :: trait`, `Point :: impl :: PartialEq`. The words `struct`, `enum`, `trait`, and `impl` are keywords that follow `::`.

---

## Directive sigils (forward model)

- `#...` compile-time directives (`#import`, `#inline`, `#derive`, etc.)
- `@...` runtime directives (`@spawn`, `@send`, `@receive`, `@reply`)

MVP does not require directive-heavy semantics; this split is a language direction to keep compile-time and runtime behavior visually distinct.

---

## Variables and blocks

```phoenix
const x = 1;
var y: s32 = 2;
y = y + 1;

const z = {
  const a = 1;
  a + 2
};
```

- `const` is immutable.
- `var` is mutable.
- Block trailing expression (without `;`) is the block value.

---

## Functions

Shape:

```phoenix
name :: (params) => ReturnType { body };
name2 :: (params) { body };      // return type defaults to ()
```

Examples:

```phoenix
add :: (a: s32, b: s32) => s32 { a + b };
log_value :: (n: s32) { /* side effect */ };
```

---

## Types and declarations

Struct and enum names use the same `::` declaration form as functions:

```phoenix
Point :: struct { x: s32, y: s32, }
Packet :: struct([u8; 16]);
Empty :: struct;

Token :: enum
{
  Eof,
  Number(s32),
  Bytes([u8; 4]),
}

type UserId = u64;
```

Struct literal and update syntax:

```phoenix
const p1 = Point { x: 1, y: 2 };
const p2 = Point { x: 3, ..p1 };
```

---

## Trait and impl syntax

Traits and impls use the same `::` declaration form as structs and enums:

```phoenix
PartialEq :: trait
{
  eq :: (self: &Self, other: &Self) => bool;
}

Point :: impl :: PartialEq
{
  eq :: (self: &Self, other: &Self) => bool
  {
    self.x == other.x && self.y == other.y
  };
}

Point :: impl
{
  length_squared :: (self: &Self) => s32
  {
    self.x * self.x + self.y * self.y
  };
}
```

---

## Explicit casts

No implicit numeric widening or narrowing. Use postfix `as`:

```phoenix
const n: u8 = 42 as u8;
const wide: s64 = 100 as s64;
```

Cast binds tighter than assignment (`=`, `+=`, …) and looser than unary operators.

---

## Control flow and pattern matching

```phoenix
if cond { a } else { b };
while cond { work(); };
loop { break; };
for item in items { process(item); };
```

```phoenix
match n
{
  0 => 1,
  _ => 2,
};
```

`given` is shorthand for single-pattern matching:

```phoenix
given Some(v) = maybe_value { use(v); }
```

---

## Operators


| Category          | Operators                    |
| ----------------- | ---------------------------- |
| Arithmetic        | `+` `-` `*` `/` `%` `**`     |
| Bitwise           | `&` `|` `^` `<<` `>>` `~`    |
| Comparison        | `==` `!=` `<` `<=` `>` `>=`  |
| Logical           | `&&` `||` `!`                |
| Assignment        | `=` `+=` `-=` `*=` `/=` `%=` |
| References (expr) | `&x` `&mut x` `*ptr`         |
| Cast              | `expr as Type`               |
| Error propagation | `?`                          |


---

## Literal forms


| Kind        | Examples                                    |
| ----------- | ------------------------------------------- |
| Integer     | `0`, `42`, `1_000`, `0xff`, `0b1010`, `42u` |
| Float       | `3.14`, `1e10`, `2.5f64`                    |
| Boolean     | `true`, `false`                             |
| Byte char   | `b'a'`, `b'\n'`                             |
| Byte string | `b\"abc\"`, `b\"A\\x0A\"`                   |


---

## Modules and imports

Files are modules. `pub` exports declarations.

```phoenix
#import core::mem
#import app::math::{add, mul}
```

---

## Precedence (highest -> lowest)

1. `.` `()` `[]`
2. unary `-` `!` `~` `*` `&` `&mut`
3. `*` `/` `%`
4. `+` `-`
5. `<<` `>>`
6. `&`
7. `^`
8. `|`
9. `==` `!=` `<` `<=` `>` `>=`
10. `&&`
11. `||`
12. `..` `..=`
13. cast (`as`)
14. assignment (`=`, `+=`, ...)

---

## Formal reference

Use [grammar.ebnf](grammar.ebnf) as the source of truth for parser and lexer implementation details.
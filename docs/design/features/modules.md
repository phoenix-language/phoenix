# Modules and imports

Phoenix has no `mod { ... }` or `use` blocks. Files are modules. Imports use the `#import` compile-time directive.

---

## File paths

The compiler maps each source file to a path from the project root and package layout (e.g. `std/http/request.phx` → `std::http::request`).

- Items at file scope are private to that module by default
- `pub` marks an item as exportable — other files may `#import` it
- Paths use `::` as separator: `std::collections::HashMap`

---

## Defining and consuming code

```
// std/http/request.phx

pub Request :: struct
{
  method: [u8; 4],
  path: [u8; 8],
}

handle :: (req: Request) => Response
{
  // ...
};
```

```
// app/main.phx

#import std::http::Request
#import std::http::handle

main :: () =>
{
  const req = Request { method: [71u, 69u, 84u, 0u], path: [47u, 104u, 101u, 97u, 108u, 116u, 104u, 0u] };
  handle(req);
};
```

You can always refer to an item by its full path without importing (`std::http::Request`), but `#import` brings names into local scope for convenience.

---

## Import forms

| Form | Effect |
|---|---|
| `#import path::Item` | `Item` is in scope by its name |
| `#import path::{A, B, C}` | Multiple items in scope |
| `#import path::*` | All `pub` items from that module (use sparingly) |

The path must resolve to a `pub` item (or a module for `::*`). Importing a private item is a compile error.

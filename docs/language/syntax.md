# Language syntax

FastLang source is UTF-8 text. Statements commonly end at a closing brace or
semicolon; blocks use braces. The language uses `->` for block-style control
flow and for return types in declarations.

## Comments

```fast
// A line comment
/* A block comment */
```

## Names and visibility

Identifiers name variables, functions, types, and namespaces. Public
declarations use `public`; declarations without it are private to their module.
Namespaces are accessed with `::`:

```fast
public fn greet() -> void { }
Math::add(1, 2);
```

## Declarations

```rust
int32 count = 0;
name := "FastLang";

fn main() -> int32 {
    return 0;
}
```

The `:=` form declares a new inferred variable. It is not a general
reassignment operator.

## Literals and casts

Numeric literals may be decimal, hexadecimal, octal, or binary:

```rust
int32 a = 42;
int32 b = 0xff;
int32 c = 0o77;
int32 d = 0b1010;
```

Explicit casts use `as`; the standard library also exposes the generic `cast`
method:

```rust
int32 whole = 3.9 as int32;
int32 other = cast<int32>(3.9); // cast is a compile-time method
```
`cast` allow you to cast any compatible type to any other type only work at compile time.
`as` is a runtime cast that can be used in any context.
## Operators

Arithmetic, comparison, boolean, assignment, indexing, member access, and
address/reference operators are supported. Types can provide operator behavior
through the language's handle system where that behavior is defined for the
type.


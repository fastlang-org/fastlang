# Macros

Macros are pattern-based language extensions. They are selected by syntax and delimiter shape, then expanded before final backend generation when their body requests AST injection.

## Declaration

A simple macro can have ordinary parameters:

```fast
macro log_value(value: str) -> {
    println(value);
}
```

Pattern macros describe the syntax that follows their name:

```fast
macro _wrap_ @[name: str]#{inner: str} -> {
    return in {
        inner;
    };
}
```

The delimiter is part of the macro's contract. The parser distinguishes direct parameters, parenthesized, bracketed, braced, angled, pipe, universal, and excluded-delimiter patterns where those forms are declared.

## AST injection with `return in`

`return in { ... }` explicitly returns code to be inserted at the call site:

```fast
public macro stop = () -> {
    return in {
        break;
    }
}
```

This is how a macro can introduce a statement such as `break` into the caller's loop. The injected code is analyzed as part of the resulting program.

## Runtime macros and compile-time macros

Not every macro is automatically a compile-time function. A normal macro may participate in generated runtime code. Compile-time behavior must be expressed through the explicit compile-time mechanisms and injection forms documented in [Compile-time programming](compile-time.md).

## Public macros and modules

`public macro` exports its pattern to importing modules. The loader shares public macro patterns with parsers created for dependent modules. Macro names can be overloaded when their patterns are distinguishable.

## Restrictions

Macro parameters that represent blocks or objects must use an explicit delimiter. This prevents the parser from confusing a block/object argument with an ordinary expression. The expanded AST must still be valid in the scope where the macro is invoked.


# Modules, imports, and visibility

## Imports

```fast
import std::fmt;
import std::http;
import my_package::auth;
```

The loader resolves standard modules, project modules, and package modules. Module names use `::` separators.

## Public declarations

Declarations are private by default. Mark declarations intended for consumers as `public`:

```fast
public fn connect() -> void { }
public blueprint User -> { }
```

Imported code can access public functions, types, blueprints, namespaces, and macro patterns. Private implementation details remain local to the module.

## Namespaces

```fast
Network::connect();
MathOps::NestedOps::mul(2, 3);
```

`using` can bring a module or type namespace into the current scope where the declaration supports it.

## Native modules

Native headers and declarations use the FFI forms described in [Foreign functions](ffi.md). A module can expose a FastLang wrapper around a native API so application code does not depend on raw C declarations.


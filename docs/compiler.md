# Compiler and command-line interface

## Invocation

The compiler executable is commonly used as `fast` (or `fast_lang`). It can
run a project command or compile a source file directly:

```text
fast run
fast run src/main.fast
fast build
fast src/main.fast
```

Common commands:

| Command | Purpose |
| --- | --- |
| `fast new NAME` | Create a project with a `Fast.toml` manifest |
| `fast init` | Initialize a manifest in the current directory |
| `fast run` | Compile and execute the project entry point |
| `fast build` | Compile without executing |
| `fast clean` | Remove generated build artifacts |
| `fast lib list` | Show configured native library paths |
| `fast lib add PATH` | Add a global native library search path |
| `fast pkg add NAME` | Add a package dependency |
| `fast pkg install` | Resolve and install package dependencies |

Use `fast --help` for the complete option list.

## Compiler options

Important options include:

```text
-b, --backend cpp|cranelift
-I, --include DIRECTORY
-o, --output PATH
--emit-cpp
--emit-ast FILE
--emit-ir
--aot
--target TARGET
--debug
--clean
```

The exact backend set is a compiler policy and may change independently from
the FastLang language syntax. The language documentation should not assume a
particular generated representation.

## Compilation pipeline

Conceptually, compilation proceeds through these stages:

1. Source loading and module resolution.
2. Lexing into FastLang tokens.
3. Parsing into the FastLang AST.
4. Declaration and macro registration.
5. Compile-time evaluation and AST expansion where requested.
6. Semantic analysis and type checking.
7. Reachability and generated-code preparation.
8. Backend code generation and native linking.

Compile-time expansion happens before final backend generation. A macro or
compile-time expression must therefore produce a valid result for the later
semantic and backend stages.

## Diagnostics

Compiler diagnostics identify the category of failure and, when available, the
source location. Typical categories are syntax errors, semantic/type errors,
compile-time errors, native compilation errors, and linker errors.

`--debug` enables compiler tracing. `--debug-errors` asks the analyzer to
collect more semantic errors before stopping.


# FastLang

FastLang is a statically typed native programming language designed around explicit types, expressive scopes, compile-time programming, native interoperability, and practical systems development.

The repository contains the compiler, standard library, project tooling, and the language documentation. FastLang is experimental and its syntax is evolving toward a stable release.

## About

FastLang combines:

- statically checked primitive, generic, object, enum, and collection types
- explicit pointers, references, mutable references, and cleanup capabilities
- functions, methods, overloads, generics, lambdas, and variadic calls
- structured control flow, pattern matching, and state machines
- pattern-based macros with explicit AST injection
- compile-time reflection and validation through the @compile namespace
- C-compatible foreign functions and native library linking
- standard-library support for I/O, networking, documents, web applications, time, threads, and SQLite

## Get started

### Requirements

Building the compiler from source currently requires:

- Rust toolchain, edition 2021
- a native compiler/toolchain for the selected backend
- a C/C++ development environment for the current runtime path

### Build the compiler

```text
git clone <repository-url>
cd fast_lang
cargo build --release
```

Place the resulting executable on PATH as fast, or invoke the compiler binary directly.

### Create and run a project

```text
fast new hello -y
cd hello
fast run
```

A project normally contains:

```text
hello/
├── Fast.toml
└── src/
    └── main.fast
```

### Minimal program

```fast
import std::fmt;

fn main() -> int32 {
    println("Hello, FastLang!");
    return 0;
}
```

## Language tour

### Functions and inference

```fast
fn identity<T>(value: T) -> T {
    return value;
}

name := "FastLang";
int32 answer = identity(42);
```

### Objects and implementations

```fast
struct Point {
    int32 x;
    int32 y;
}

impl Point -> {
    fn label() -> str {
        return "(" + this.x + ", " + this.y + ")";
    }
}
```

### Enums and matching

```fast
enum Status {
    Pending,
    Active(int32),
    Done(str),
}

fn describe(status: Status) -> void {
    match (status) -> {
        Pending => println("waiting");
        Active(id) => println("active: ", id);
        Done(message) => println(message);
        _ => println("unknown");
    }
}
```

### Compile-time programming

```fast
fn require_printable<T>(value: T) -> void {
    @compile {
        if (!typeof(value).printable()) {
            @compile::throw new Error("value is not printable");
        }
    }(value);
}
```

### Native interoperability

```fast
extern @c "raylib.h" :: {
    fn InitWindow(width: int32, height: int32, title: str);
}
```

## Project workflow

```text
fast new NAME       Create a project
fast init           Initialize Fast.toml
fast run            Compile and run
fast build          Compile without running
fast clean          Remove build artifacts
fast --help         Show compiler and CLI options
```

Project configuration lives in `Fast.toml`. It defines package metadata, the entry point, backend selection, dependencies, scripts, and native link flags.

## Documentation

The complete, feature-by-feature language and compiler documentation is available in the [FastLang documentation index](docs/index.md).

The documentation is organized into independent pages for:

- [Syntax and declarations](docs/language/syntax.md)
- [Types and values](docs/language/types.md)
- [Functions and lambdas](docs/language/functions.md)
- [Control flow and loops](docs/language/control-flow.md)
- [Objects and data types](docs/language/objects.md)
- [Enums and pattern matching](docs/language/enums.md)
- [Macros](docs/language/macros.md)
- [Compile-time programming](docs/language/compile-time.md)
- [Memory and cleanup](docs/language/memory.md)
- [Projects and Fast.toml](docs/projects.md)
- [Compiler CLI](docs/compiler.md)
- [Standard library](docs/stdlib.md)

For the short overview, see [language-guide.md](docs/language-guide.md). Internal design notes are kept separately in [docs/internal/todo.fast](docs/internal/todo.fast) and are not language guarantees.

## Backend direction

The project is moving toward a native C99 backend as the primary backend for the next compiler phase. Backend implementation details are intentionally kept separate from the language reference so that the language documentation remains stable while the compiler backend evolves.

## License

FastLang is distributed under the license declared by the project.

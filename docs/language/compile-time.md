# Compile-time programming

FastLang has an explicit compile-time namespace and validation-block syntax. Compile-time code is evaluated by the compiler before backend generation.

## Intrinsics

Common compiler services include:

```fast
type t = @compile::typeof(value);
uint32 size = @compile::sizeof(value);
int32 casted = @compile::cast<int32>(12.5);
Backend backend = @compile::backend();
bool runtime = @compile::runtime();
System system = @compile::system();
```

The standard library wraps several of these operations as ordinary-looking functions and methods.

## Validation blocks

Use a validation block when a call must be checked using compile-time values:

```fast
fn require_printable<T>(value: T) -> void {
    @compile {
        if (!typeof(value).printable()) {
            @compile::throw new Error("value is not printable");
        }
    }(value);
}
```

The validation block is not emitted as runtime code. Its result is used to accept, reject, or specialize the surrounding program.

## Compile-time diagnostics

`@compile::throw` stops compilation with a language-level diagnostic. `@compile::print` writes a compiler-time diagnostic message. These operations are separate from runtime I/O and runtime exceptions.

## Compile-time versus runtime

A compile-time expression may only depend on values available to the compiler and on compiler services explicitly supported by the intrinsic. Runtime input, network state, and other dynamic values cannot be used as compile-time facts.

The language is being extended toward mixed compile-time/runtime APIs. Until a feature is specified, use explicit `@compile` code instead of assuming that a normal function will automatically execute during compilation.


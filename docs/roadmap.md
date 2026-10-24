# Backend policy and roadmap

## Language contract versus backend

FastLang syntax and semantics are the language contract. Code generation, native linking, and target-specific runtime details are backend concerns and must be documented separately.

## Current direction

The project is moving toward a native C99 backend as the primary backend for the next compiler phase. This is intended to improve portability, toolchain availability, and compilation speed. Existing C++ runtime and generator code is useful as an implementation reference during the transition.

The Cranelift backend is not the focus of the current development phase. Its documentation should remain limited to supported behavior until its future role is decided.

## Feature lifecycle

A language feature should move through these stages:

1. Design note in the internal TODO or architecture notes.
2. Parser and semantic definition.
3. Backend/runtime implementation.
4. Stable syntax and error behavior.
5. Dedicated public documentation page.

Removing or merging a feature should normally require updating only its feature page, the documentation index, and any cross-references. For example, if blueprints are eventually merged into structs, the object page and its links can describe the new model without changing the documentation for functions, loops, modules, or the compiler CLI.

## Not public guarantees

Internal TODO examples, generated C/C++ details, experimental macro patterns, and unsupported backend capabilities are not public guarantees until they are promoted into the relevant feature page.


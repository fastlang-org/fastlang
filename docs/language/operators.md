# Operators and handles

FastLang has built-in arithmetic, comparison, boolean, indexing, member, address, and assignment operators. Types can customize supported behavior through handles.

## Handle access

The handle-access operator is written with -.: 

    value-.display();
    object-.drop();

This is distinct from ordinary method access. A handle is a capability declared by or associated with a type.

## Supported capabilities

Depending on the type, handles can describe:

- arithmetic and compound assignment
- increment and decrement
- boolean negation and numeric negation
- indexing and property access
- construction and default values
- display and string conversion
- copy, drop, throw, and error behavior
- access to a wrapped or foreign value

The analyzer checks whether an operation is supported before code generation. A type that does not expose the required capability cannot silently receive an unrelated operator.


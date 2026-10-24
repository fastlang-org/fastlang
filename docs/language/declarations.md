# Declarations, aliases, and scopes

## Type aliases

A type alias gives a project-facing name to an existing type:

    define UserID = int32;
    UserID id = 42;

Aliases are resolved during parsing and semantic analysis; they do not create a new runtime representation.

## Variable declarations

    int32 count = 0;
    str name;
    count = 1;

Declarations can appear in permitted scopes. A variable declared in an inner block is not visible after that block ends.

## Scope blocks

Named blocks and namespaces group declarations. public, private, and static control which declarations are visible and when they are associated with an object/type scope.

## Static sections

Struct-like and object-like types can define static members or a static block. Static declarations belong to the type rather than an individual instance.


# Objects, structs, classes, and blueprints

## Structs

Structs describe C-like data with named fields:

```fast
struct Point {
    int32 x;
    int32 y;
}

Point p = Point{x: 10, y: 20};
```

## Classes

Classes support object-style construction and encapsulation where the class declaration provides those capabilities:

```fast
class Counter {
    int32 value;
}

Counter c = new Counter();
```

## Blueprints

Blueprints are data-first object declarations with fields, generic parameters, and separate `impl` blocks:

```fast
blueprint User -> {
    int32 id;
    str name;
}

impl User -> {
    fn display() -> str {
        return this.name;
    }
}
```

Blueprints are currently documented as a distinct language construct. Their future relationship with structs is a language-design decision; code should not assume that the two declarations are interchangeable.

## `impl` blocks

`impl` keeps behavior separate from data declarations:

```fast
impl Point -> {
    fn label() -> str {
        return "(" + this.x + ", " + this.y + ")";
    }
}
```

Methods and supported handles can be added through implementation blocks.

## Handles

Handles are type capabilities used by the language runtime and analyzer. Common roles include `display`, `default`, `drop`, access, cast, copy, and operator behavior. A type's handles are part of its semantic contract; they are not ordinary methods in every context.


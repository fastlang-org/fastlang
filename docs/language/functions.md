# Functions, methods, generics, and lambdas

## Functions

```fast
fn add(a: int32, b: int32) -> int32 {
    return a + b;
}
```

Functions can return values or `void`. A return type may be inferred in
supported implicit-return forms, but explicit return types are recommended for
public APIs.

## Default arguments and overloading

```fast
fn scale(value: int32, factor := 2) -> int32 {
    return value * factor;
}

fn parse(value: int32) -> str { return "integer"; }
fn parse(value: float64) -> str { return "float"; }
```

Overload resolution uses the callable name and argument types. Generic and
concrete overloads can coexist when their signatures are distinguishable.

## Generic functions and methods

```fast
fn identity<T>(value: T) -> T {
    return value;
}

method first<T, U>(a: T, _b: U) -> T {
    return a;
}
```

Type parameters can be inferred or supplied explicitly:

```fast
int32 n = identity(10);
str s = identity<str>("text");
```

## Methods

Methods may be declared in an `impl` block or assigned as callable values.
Inside an instance method, `this` refers to the receiver.

## Lambdas and function values

```fast
lambda<(int32) -> int32> increment = |x| x + 1;
int32 result = increment(4);
```

Anonymous functions can be passed to APIs or invoked immediately:

```fast
int32 result = (fn(a: int32, b: int32) -> int32 {
    return a + b;
})(2, 3);
```

## Variadic parameters and spreading

Variadic parameters use `...T` and can be expanded with `...` at a call site:

```fast
fn print_all<...T>(...args: ...T) -> void {
    for arg in args {
        println(arg);
    }
}

print_all("count", 3, true);
```


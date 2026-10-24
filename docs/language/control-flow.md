# Control flow and loops

## `if` and `else`

```fast
if (score >= 50) {
    println("passed");
} else {
    println("try again");
}
```

The block-arrow form is also used by control-flow constructs:

```fast
if (ready) -> {
    start();
}
```

## `while`

```fast
while (remaining > 0) -> {
    remaining -= 1;
}
```

## `for`

The C-style form contains initialization, condition, and update expressions:

```fast
for (int32 i = 0; i < 10; i += 1) {
    println(i);
}
```

## `for in`

```fast
for item in values {
    println(item);
}
```

The range syntax is supplied by the standard range macro:

```fast
for item in range(0, 10) {
    println(item);
}
```

## `break` and `continue`

`break` exits the nearest loop. `continue` skips to the next iteration.

```fast
while (true) -> {
    if (done) {
        break;
    }
    continue;
}
```

## `match`

`match` selects a branch by structural pattern. See [Enums and pattern
matching](enums.md) for variants, bindings, and fallback patterns.

## `try` and `catch`

```fast
try {
    risky_operation();
} catch (error) -> {
    println(error.message);
}
```


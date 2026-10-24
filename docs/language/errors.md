# Errors, exceptions, `Option`, and `Result`

## Throwing and catching

Errors can be created and thrown, then handled with `try` and `catch`:

```fast
fn read_required(value: str) -> str {
    if (value.is_empty()) {
        throw new Error("value is required");
    }
    return value;
}

try {
    println(read_required(""));
} catch (error) -> {
    println(error.message);
}
```

The standard `Error` type carries a message and can be extended with domain-specific error types.

## `Result<T>`

`Result<T>` represents success or failure as a value:

```fast
fn load() -> Result<int32> {
    return Ok(42);
}

Result<int32> result = load();
if (result.is_Err()) {
    println(result.get_or(0));
}
```

`get()` returns the success value or raises the stored error. `get_or(default)` returns a fallback value for an error.

## Compile-time errors

`@compile::throw` reports a compiler error during compile-time evaluation. It is distinct from a runtime `throw` and is not caught by an ordinary runtime `try`/`catch` block.


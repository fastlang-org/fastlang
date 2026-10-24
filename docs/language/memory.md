# Memory, pointers, references, and cleanup

FastLang makes value and aliasing behavior visible in the type system. The compiler tracks ordinary values, heap-backed values, raw pointers, references, and mutable references separately.

## Allocation

```fast
int32* number = new int32(10);
int32 values[] = new int32[](8);
```

Use `new` for explicit allocation and construction. Arrays are heap-backed collections even when their element values are primitive.

## Addresses and mutation

```fast
int32 value = 10;
int32* address = &value;
value = 20;
```

`ref<T>` expresses a reference to a value. `mutRef<T>` expresses a mutable reference where the operation permits mutation:

```fast
fn increment(value: mutRef<int32>) -> void {
    value += 1;
}
```

## Deletion and drop behavior

`del` is the explicit deletion operation. Object-like types can provide a `drop` handle so cleanup behavior is type-specific. The compiler validates that stack values, borrowed references, and already-deleted values are not deleted incorrectly.

The exact automatic cleanup policy is still being refined. Public code should use the documented ownership form and avoid depending on undocumented ordering between user `del` operations and generated cleanup.


# Types and values

## Primitive types

The primitive family includes signed and unsigned integers, floating-point
values, booleans, characters, Unicode scalar values, strings, `void`, and
platform-sized integers:

```fast
int8 a;
int32 count;
int64 large;
uint32 flags;
usize length;
isize offset;
float32 x;
float64 y;
bool ready;
char byte_char;
uchar code_point;
str text;
```

`char` is the narrow character used by the current string and C interfaces.
`uchar` represents one Unicode code point and is encoded as UTF-8 when printed.

## Arrays

An array type is written with brackets after its element type or name:

```fast
int32 values[] = [1, 2, 3];
int32 values[] = new int32[](3);
```

Arrays have a length and support indexing. Nested arrays represent nested
collections.

## Pointers and references

```fast
int32 value = 10;
int32* pointer = &value;
mutRef<int32> mutable_ref = pointer;
```

Pointers expose address-level access. `ref<T>` and `mutRef<T>` communicate
borrowed or mutable aliasing to the analyzer and runtime. The distinction is
important for deletion and mutation.

## Type values and reflection

`type` values can be obtained with `typeof` and inspected through compile-time
or type handles:

```fast
type value_type = typeof(value);
if (value_type.primitive()) { }
```

Supported type queries include primitive, pointer, array, size, printable,
castability, fields, methods, handles, runtime status, and compile-time status.

## Inference and `auto`

`:=` infers a concrete type from an initializer. `auto` is useful in APIs where
the compiler must infer a value type, but public interfaces should prefer an
explicit type when the contract matters.


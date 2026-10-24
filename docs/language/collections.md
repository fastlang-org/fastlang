# Collections and strings

## Arrays

```fast
str names[] = ["Ada", "Grace", "Hakim"];
println(names[0]);
println(names.len);
```

Arrays are homogeneous. They can be allocated with `new T[](size)` and passed
to generic or variadic functions.

## Strings

`str` is the standard text sequence used by the standard library:

```fast
str greeting = "Hello";
str message = greeting + ", FastLang!";
println(message);
```

String operations include indexing, length, emptiness, comparison, and
conversion through the standard string implementation. `char` indexing is a
byte/narrow-character operation; it should not be treated as Unicode grapheme
segmentation.

## Character operations

```fast
char c = 'a';
bool letter = c.is_alpha();
char upper = c.to_upper();
```

`uchar` is for a single Unicode scalar value. Convert or collect Unicode data
explicitly when handling UTF-8 text.

## Standard collections

The standard library contains collection modules such as lists, maps, sets,
and ranges. Their exact APIs belong in the standard-library reference and may
be extended without changing the core array type.


# Enums and pattern matching

Enums define a closed set of variants. Variants may be empty or carry values:

```fast
enum Status {
    Pending,
    Active(int32),
    Completed(str),
}
```

An enum may also declare an underlying representation:

```fast
enum Document : str {
    ["html"] = Html(DocumentPayload),
    ["json"] = Json(DocumentPayload),
    ("raw") = Raw(DocumentPayload),
}
```

## Construction and dynamic indexing

```fast
Status current = Active(42);
Document document = Document["html"](payload);
```

An enum with a default variant can use that variant when a dynamic key does not match a registered key.

## Matching

```fast
match (current) -> {
    Pending => println("waiting");
    Active(id) => println("running: ", id);
    Completed(message) => println(message);
    _ => println("unknown");
}
```

The `_` pattern is a fallback. Variant bindings are scoped to their match arm.


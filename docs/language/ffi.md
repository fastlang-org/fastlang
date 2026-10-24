# Foreign functions and native libraries

FastLang can include native headers and declare C-compatible functions and types.

## Header imports

```fast
import @c "raylib.h";
```

## External declarations

```fast
extern @c "raylib.h" :: {
    fn InitWindow(width: int32, height: int32, title: str);
    fn CloseWindow();
}
```

Native declarations are available to the generated backend and are linked with the project's native configuration.

## Wrappers

Prefer wrapping native functions in a FastLang module:

```fast
public fn open_window(width: int32, height: int32) -> void {
    InitWindow(width, height, "FastLang");
}
```

This keeps application code independent from header names and calling details.

## Native linking

Use `package.link_flags` in `Fast.toml` and project/global library paths for native dependencies. Header availability, ABI compatibility, and linker names remain platform/toolchain concerns.


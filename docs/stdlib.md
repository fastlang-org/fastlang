# Standard library

The standard library is organized into modules imported with `std::`.

| Module | Role |
| --- | --- |
| `std::types` | Core types, arrays, references, handles, and capabilities |
| `std::io` | Files, streams, input, and low-level I/O wrappers |
| `std::fmt` | `print`, `println`, formatting, and printable validation |
| `std::string` | String object and string operations |
| `std::collections` | Lists, maps, sets, and collection helpers |
| `std::range` | Range values and range syntax macros |
| `std::math` | Numeric functions |
| `std::error` | Error and result types |
| `std::time` | Time and timer values |
| `std::mem` | Memory helpers |
| `std::thread` | Threads and synchronization primitives |
| `std::net` | Sockets and networking |
| `std::http` | HTTP server/client and document responses |
| `std::document` | File-backed document values and formats |
| `std::dom` | DOM/document tree structures |
| `std::web` | Web-oriented helper APIs |
| `std::sql` | SQLite database access |

The standard library may use compiler intrinsics, native C functions, and backend-specific implementations behind a stable FastLang-facing API.

## Printing

```fast
import std::fmt;

println("answer: ", 42);
```

The formatter checks whether values are printable during compilation. Types can provide display behavior through the supported display handle.

## File I/O

```fast
import std::io;

str content = io::read_all("notes.txt");
```

Use `std::document` when the file should be represented with its name, contents, extension, and timestamps.


# Projects and `Fast.toml`

FastLang projects are directories with a `Fast.toml` manifest. The compiler
looks for `Fast.toml` in the current directory and then in its parents. A
lowercase `fast.toml` is also accepted.

## Minimal manifest

```toml
[package]
name = "hello"
version = "0.1.0"
type = "bin"
main = "src/main.fast"
backend = "cpp"
```

`type` may be `bin` for an executable package or `module`/`lib` for a library
package. A binary defaults to `src/main.fast`; a module defaults to
`src/lib.fast` when `main` is omitted.

## Package fields

```toml
[package]
name = "my_app"
version = "0.1.0"
type = "bin"
main = "src/main.fast"
description = "An example FastLang application"
authors = ["Author Name"]
license = "MIT"
backend = "cpp"
link_flags = ["raylib"]
```

`link_flags` contains native linker flags or library names required by the
project. Their interpretation depends on the selected native toolchain.

## Scripts

Project scripts are named commands stored in the manifest:

```toml
[scripts]
dev = "fast run"
release = "fast build --clean"
```

Scripts are project conveniences; they do not introduce new language syntax.

## Dependencies and paths

```toml
[dependencies]
some_package = "1.0.0"

[paths]
packages = "packages"
libraries = ["native", "vendor"]
```

`packages` selects the project-local package directory. `libraries` adds
project-local native library search paths.

## Global compiler configuration

The compiler keeps optional global settings in `.fast/config.toml`. The path
can be overridden with `FAST_CONFIG_PATH`.

```toml
default_backend = "cpp"
cxx_compiler = "g++"
library_paths = ["C:/libs"]
std_path = "C:/fastlang/std"
packages_dir = "C:/fastlang/packages"
```

Global configuration is for toolchain and search-path defaults. Project
settings in `Fast.toml` take precedence where both are provided.


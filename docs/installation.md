# Installation

## Prerequisites

The current compiler distribution requires a Rust toolchain to build the compiler and a native C/C++ toolchain for the active backend. The project is moving toward a Pure C99 backend; installation instructions for that backend will become the release-specific default once it is shipped.

## Build from source

    git clone <repository>
    cd fast_lang
    cargo build --release

The resulting compiler binary can be placed on PATH and invoked as fast.

## Create a project

    fast new hello -y
    cd hello
    fast run

## Toolchain configuration

Use the global .fast/config.toml for the default backend, native compiler, standard-library path, package path, and native library search paths. Use Fast.toml for settings belonging to one project.

## Release packaging

A release package should contain the compiler executable, standard-library sources/runtime files, supported native headers or libraries, and a clear target-platform matrix. The exact layout is release tooling rather than language syntax.


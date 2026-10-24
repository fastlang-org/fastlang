// Compiler Intrinsics Registry for FastLang
// Centralized metadata table for compile-time built-ins and type properties

use crate::frontend::parser::ast::BaseType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntrinsicSignature {
    pub name: &'static str,
    pub min_args: usize,
    pub max_args: usize,
    pub return_type: &'static str,
    pub description: &'static str,
}

pub struct CompilerIntrinsics;

impl CompilerIntrinsics {
    pub const COMPILE_INTRINSICS: &'static [IntrinsicSignature] = &[
        IntrinsicSignature {
            name: "typeof",
            min_args: 1,
            max_args: 1,
            return_type: "type<T>",
            description: "Compile-time type reflection metadata",
        },
        IntrinsicSignature {
            name: "sizeof",
            min_args: 1,
            max_args: 1,
            return_type: "uint32",
            description: "Compile-time type size in bytes",
        },
        // cast and throw are not built-in intrinsics, they are handled by the compiler directly
        // only as is translated into fastlang_cast
        // and throw keyword is translated into fastlang_throw in the interpreter
        // see the std.fast file for the actual implementation of these intrinsics (throw and cast)
        IntrinsicSignature {
            name: "print",
            min_args: 1,
            max_args: usize::MAX,
            return_type: "void",
            description: "Compile-time diagnostic printing during build",
        },
        IntrinsicSignature {
            name: "access",
            min_args: 1,
            max_args: 2,
            return_type: "unknown",
            description: "Compile-time property access forwarding handle helper",
        },
        IntrinsicSignature {
            name: "rand",
            min_args: 0,
            max_args: 3,
            return_type: "T",
            description: "Compile-time random numeric constant generator",
        },
        IntrinsicSignature {
            name: "backend",
            min_args: 0,
            max_args: 0,
            return_type: "Backend",
            description: "Compile-time backend detection",
        },
        IntrinsicSignature {
            name: "system",
            min_args: 0,
            max_args: 0,
            return_type: "System",
            description: "Compile-time host system detection",
        },
        IntrinsicSignature {
            name: "time",
            min_args: 0,
            max_args: 0,
            return_type: "Time",
            description: "Compile-time timestamp returning Time struct",
        },
        IntrinsicSignature {
            name: "create_ident",
            min_args: 1,
            max_args: 1,
            return_type: "ident",
            description: "Compile-time identifier constructor",
        },
        IntrinsicSignature {
            name: "open",
            min_args: 1,
            max_args: 1,
            return_type: "str",
            description: "Compile-time file open and read",
        },
        IntrinsicSignature {
            name: "write",
            min_args: 2,
            max_args: 2,
            return_type: "void",
            description: "Compile-time file write",
        },
        IntrinsicSignature {
            name: "toml",
            min_args: 1,
            max_args: 1,
            return_type: "unknown",
            description: "Compile-time TOML parser helper",
        },
        IntrinsicSignature {
            name: "get_toml",
            min_args: 1,
            max_args: 1,
            return_type: "unknown",
            description: "Compile-time read and parse TOML file",
        },
    ];

    pub const TYPE_PROPERTIES: &'static [&'static str] = &[
        "printable",
        "pointer",
        "array",
        "primitive",
        "throwable",
        "shareable",
        "copyable",
        "as_str",
        "castable",
        "castable_to",
        "constant",
        "runtime",
        "comptime",
        "undefined",
        "has_handle",
        "has_method",
        "has_field",
        "size",
        "on_heap",
        "on_stack",
    ];

    pub fn find_compile_member(name: &str) -> Option<&'static IntrinsicSignature> {
        let clean_name = name
            .strip_prefix("@compile::")
            .unwrap_or(name)
            .trim_start_matches('$');
        Self::COMPILE_INTRINSICS
            .iter()
            .find(|i| i.name == clean_name)
    }

    pub fn is_compile_member(name: &str) -> bool {
        Self::find_compile_member(name).is_some()
    }

    pub fn is_type_property(prop: &str) -> bool {
        Self::TYPE_PROPERTIES.contains(&prop)
    }

    pub fn is_numeric_type(base: &BaseType) -> bool {
        matches!(
            base,
            BaseType::Int(_)
                | BaseType::UInt(_)
                | BaseType::USize
                | BaseType::ISize
                | BaseType::Float(_)
        )
    }
}

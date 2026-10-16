use crate::frontend::parser::ast::{BaseType, Flag};
use std::collections::HashSet;

/// `FastType`: Compile-time Type Representation in the Interpreter.
/// Extends `BaseType` with reflection attributes, handle capabilities, and compile-time methods.
#[derive(Debug, Clone, PartialEq)]
pub struct FastType {
    pub base_type: BaseType,
    pub flags: HashSet<Flag>,
    pub handles: Vec<String>,
    pub methods: Vec<String>,
    pub fields: Vec<String>,
    pub field_types: std::collections::HashMap<String, String>,
    pub method_types: std::collections::HashMap<String, String>,
    pub handle_types: std::collections::HashMap<String, String>,
}

impl FastType {
    pub fn new(base_type: BaseType) -> Self {
        Self {
            base_type,
            flags: HashSet::new(),
            handles: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            field_types: std::collections::HashMap::new(),
            method_types: std::collections::HashMap::new(),
            handle_types: std::collections::HashMap::new(),
        }
    }

    pub fn from_name(name: &str) -> Self {
        Self::new(BaseType::from_str(name))
    }

    pub fn with_handles(
        base_type: BaseType,
        has_display: bool,
        has_copy: bool,
        has_cast: bool,
    ) -> Self {
        let mut flags = HashSet::new();
        if has_display { flags.insert(Flag::Printable); }
        if has_copy { flags.insert(Flag::Copyable); }
        if has_cast { flags.insert(Flag::Castable); }
        Self {
            base_type,
            flags,
            handles: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            field_types: std::collections::HashMap::new(),
            method_types: std::collections::HashMap::new(),
            handle_types: std::collections::HashMap::new(),
        }
    }

    pub fn with_all_handles(
        base_type: BaseType,
        has_display: bool,
        has_copy: bool,
        has_cast: bool,
        has_throw: bool,
        has_default: bool,
        has_share: bool,
    ) -> Self {
        let mut flags = HashSet::new();
        if has_display { flags.insert(Flag::Printable); }
        if has_copy { flags.insert(Flag::Copyable); }
        if has_cast { flags.insert(Flag::Castable); }
        if has_throw { flags.insert(Flag::Throwable); }
        if has_default { flags.insert(Flag::Defaultable); }
        if has_share { flags.insert(Flag::Shareable); }
        Self {
            base_type,
            flags,
            handles: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            field_types: std::collections::HashMap::new(),
            method_types: std::collections::HashMap::new(),
            handle_types: std::collections::HashMap::new(),
        }
    }

    pub fn with_metadata(
        base_type: BaseType,
        has_display: bool,
        has_copy: bool,
        has_cast: bool,
        has_throw: bool,
        has_default: bool,
        has_share: bool,
        handles: Vec<String>,
        methods: Vec<String>,
        fields: Vec<String>,
    ) -> Self {
        let mut flags = HashSet::new();
        if has_display { flags.insert(Flag::Printable); }
        if has_copy { flags.insert(Flag::Copyable); }
        if has_cast { flags.insert(Flag::Castable); }
        if has_throw { flags.insert(Flag::Throwable); }
        if has_default { flags.insert(Flag::Defaultable); }
        if has_share { flags.insert(Flag::Shareable); }
        Self {
            base_type,
            flags,
            handles,
            methods,
            fields,
            field_types: std::collections::HashMap::new(),
            method_types: std::collections::HashMap::new(),
            handle_types: std::collections::HashMap::new(),
        }
    }

    pub fn with_full_metadata(
        base_type: BaseType,
        has_display: bool,
        has_copy: bool,
        has_cast: bool,
        has_throw: bool,
        has_default: bool,
        has_share: bool,
        handles: Vec<String>,
        methods: Vec<String>,
        fields: Vec<String>,
        field_types: std::collections::HashMap<String, String>,
        method_types: std::collections::HashMap<String, String>,
        handle_types: std::collections::HashMap<String, String>,
    ) -> Self {
        let mut flags = HashSet::new();
        if has_display { flags.insert(Flag::Printable); }
        if has_copy { flags.insert(Flag::Copyable); }
        if has_cast { flags.insert(Flag::Castable); }
        if has_throw { flags.insert(Flag::Throwable); }
        if has_default { flags.insert(Flag::Defaultable); }
        if has_share { flags.insert(Flag::Shareable); }
        Self {
            base_type,
            flags,
            handles,
            methods,
            fields,
            field_types,
            method_types,
            handle_types,
        }
    }

    pub fn has_flag(&self, flag: &Flag) -> bool {
        self.flags.contains(flag)
    }

    pub fn is_primitive(&self) -> bool {
        self.base_type.is_primitive()
    }

    pub fn is_pointer(&self) -> bool {
        self.base_type.is_pointer()
    }

    pub fn on_heap(&self) -> bool {
        self.base_type.on_heap()
    }

    pub fn on_stack(&self) -> bool {
        self.base_type.on_stack()
    }

    pub fn is_array(&self) -> bool {
        self.base_type.is_array()
    }

    pub fn is_constant(&self) -> bool {
        self.base_type.as_str().starts_with("const ")
    }

    pub fn is_runtime(&self) -> bool {
        false
    }

    pub fn is_comptime(&self) -> bool {
        true
    }

    pub fn is_undefined(&self) -> bool {
        matches!(self.base_type, BaseType::Unknown)
            || self.base_type.as_str() == "undefined"
            || self.base_type.as_str() == "Unknown"
    }

    pub fn has_handle(&self, name: &str) -> bool {
        let clean = name.trim();
        self.handles.iter().any(|h| h == clean)
            || (clean == "display" && self.flags.contains(&Flag::Printable))
            || (clean == "copy" && self.flags.contains(&Flag::Copyable))
            || (clean == "cast" && self.flags.contains(&Flag::Castable))
            || ((clean == "throw" || clean == "$throw") && self.flags.contains(&Flag::Throwable))
            || (clean == "default" && self.flags.contains(&Flag::Defaultable))
            || (clean == "share" && self.flags.contains(&Flag::Shareable))
            || (clean == "as_str" && (self.printable()))
    }

    pub fn has_method(&self, name: &str) -> bool {
        let clean = name.trim();
        self.methods.iter().any(|m| m == clean) || (clean == "as_str" && (self.printable()))
    }

    pub fn has_field(&self, name: &str) -> bool {
        let clean = name.trim();
        self.fields.iter().any(|f| f == clean)
    }

    pub fn has_handle_query(&self, name_or_type: &str, target_type: Option<&str>) -> bool {
        let clean = name_or_type.trim();
        if let Some(t) = target_type {
            let t_clean = t.trim();
            if let Some(actual_t) = self.handle_types.get(clean) {
                return actual_t == t_clean || type_matches(actual_t, t_clean);
            }
            return false;
        }
        if self.has_handle(clean) {
            return true;
        }
        self.handle_types
            .values()
            .any(|v| v == clean || type_matches(v, clean))
    }

    pub fn has_method_query(&self, name_or_type: &str, target_type: Option<&str>) -> bool {
        let clean = name_or_type.trim();
        if let Some(t) = target_type {
            let t_clean = t.trim();
            if let Some(actual_t) = self.method_types.get(clean) {
                return actual_t == t_clean || type_matches(actual_t, t_clean);
            }
            return false;
        }
        if self.has_method(clean) {
            return true;
        }
        self.method_types
            .values()
            .any(|v| v == clean || type_matches(v, clean))
    }

    pub fn has_field_query(&self, name_or_type: &str, target_type: Option<&str>) -> bool {
        let clean = name_or_type.trim();
        if let Some(t) = target_type {
            let t_clean = t.trim();
            if let Some(actual_t) = self.field_types.get(clean) {
                return actual_t == t_clean || type_matches(actual_t, t_clean);
            }
            return false;
        }
        if self.has_field(clean) {
            return true;
        }
        self.field_types
            .values()
            .any(|v| v == clean || type_matches(v, clean))
    }

    pub fn printable(&self) -> bool {
        if matches!(
            self.base_type,
            BaseType::Unknown | BaseType::Auto | BaseType::TypeExpr(_) | BaseType::GenericParam(_)
        ) {
            return true;
        }
        let s = self.as_str();
        let name = self.base_type.get_name();
        if s.starts_with("type<")
            || s == "auto"
            || s == "unknown"
            || s == "char[]"
            || matches!(self.base_type, BaseType::GenericParam(_))
            || (s.starts_with("blueprint::") && name.len() == 1)
        {
            return true;
        }
        self.is_primitive() || self.flags.contains(&Flag::Printable)
    }

    pub fn throwable(&self) -> bool {
        self.flags.contains(&Flag::Throwable)
    }

    pub fn shareable(&self) -> bool {
        self.flags.contains(&Flag::Shareable)
    }

    pub fn copyable(&self) -> bool {
        self.is_primitive() || self.flags.contains(&Flag::Copyable)
    }

    pub fn castable(&self) -> bool {
        match &self.base_type {
            BaseType::Int(_)
            | BaseType::UInt(_)
            | BaseType::USize
            | BaseType::ISize
            | BaseType::Float(_)
            | BaseType::Bool
            | BaseType::Char
            | BaseType::UChar
            | BaseType::Str
            | BaseType::UStr
            | BaseType::Number => true,
            _ if self.is_pointer() => true,
            _ => self.flags.contains(&Flag::Castable),
        }
    }

    pub fn castable_to(&self, target: &FastType) -> bool {
        if self.is_primitive() && target.is_primitive() && !target.is_array() {
            return true;
        }
        if (self.base_type == BaseType::Str && target.base_type == BaseType::UStr)
            || (self.base_type == BaseType::UStr && target.base_type == BaseType::Str)
        {
            return true;
        }
        if self.base_type == target.base_type {
            return true;
        }
        if self.is_pointer() && target.is_pointer() {
            return true;
        }
        self.flags.contains(&Flag::Castable)
    }

    pub fn as_str(&self) -> String {
        self.base_type.as_str()
    }

    pub fn size(&self) -> u32 {
        self.base_type.size()
    }
}

fn normalize_type_str(s: &str) -> &str {
    match s {
        "int" => "int32",
        "uint" => "uint32",
        "float" => "float32",
        _ => s,
    }
}

fn type_matches(actual: &str, expected: &str) -> bool {
    let a = actual.trim();
    let e = expected.trim();
    if a == e {
        return true;
    }
    normalize_type_str(a) == normalize_type_str(e)
}

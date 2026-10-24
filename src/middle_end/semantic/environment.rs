use crate::frontend::parser::ast::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

// ─────────────────────────────────────────────────────────────────────────────
// BlueprintData — encapsulates composite types (Custom, Class, Struct, Enum)
// Constructed from TypeMetadata or directly from the AST Declaration
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct FnSignature {
    pub name: String,
    pub generics: Vec<BaseType>,
    pub params: Vec<Param>,
    pub return_type: BaseType,
    pub is_virtual: bool,
    pub is_abstract: bool,
}

#[derive(Debug, Clone)]
pub struct BlueprintData {
    pub name: String,
    pub fields: HashMap<String, BaseType>,
    pub methods: HashMap<String, FnSignature>,
    pub handles: HashSet<HandleMethods>,
    pub handle_signatures: HashMap<String, FnSignature>,
    pub settings: HashSet<Setting>,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub variants: Vec<EnumVariant>,
    pub is_class: bool,
    pub private_fields: HashSet<String>,
    pub private_methods: HashSet<String>,
}

impl BlueprintData {
    pub fn new(name: impl Into<String>) -> Self {
        BlueprintData {
            name: name.into(),
            fields: HashMap::new(),
            methods: HashMap::new(),
            handles: HashSet::new(),
            handle_signatures: HashMap::new(),
            settings: HashSet::new(),
            generics: Vec::new(),
            params: Vec::new(),
            variants: Vec::new(),
            is_class: false,
            private_fields: HashSet::new(),
            private_methods: HashSet::new(),
        }
    }

    /// Checks if this blueprint defines a specific handle method
    pub fn has_handle(&self, h: HandleMethods) -> bool {
        self.handles.contains(&h)
    }

    /// Checks if this blueprint is marked as shareable (has handle share)
    pub fn is_shareable(&self) -> bool {
        self.has_handle(HandleMethods::Share)
    }

    /// Defines a handle with its signature in the dedicated handle storage
    pub fn define_handle(&mut self, h: HandleMethods, sig: FnSignature) {
        self.handles.insert(h);
        self.handle_signatures.insert(h.as_str().to_string(), sig);
    }

    /// Checks if the specified handle accepts a parameter of the given type
    /// Used to verify operator overloading compatibility
    pub fn handle_accepts_type(&self, h: HandleMethods, value_type: &str) -> bool {
        let fn_name = h.as_str();
        let maybe_sig = self.handle_signatures.get(fn_name);
        if let Some(sig) = maybe_sig {
            if sig.params.is_empty() {
                // Handle without params accepts anything
                return true;
            }
            let param_type = sig.params[0].type_node.as_str();
            let clean_p = param_type
                .trim_start_matches("class::")
                .trim_start_matches("struct::")
                .trim_start_matches("blueprint::")
                .trim_start_matches('.')
                .trim_start_matches("...");

            // If param_type is generic (T, U, ...), accept unconditionally
            if clean_p == "T"
                || clean_p == "U"
                || clean_p == "V"
                || self.generics.iter().any(|g| {
                    let clean_g = g.trim_start_matches('.').trim_start_matches("...");
                    clean_g == clean_p
                })
            {
                return true;
            }
            // Accept if types match or if it is an array
            if param_type == value_type {
                return true;
            }
            if (param_type.starts_with("int") || param_type == "isize" || param_type == "byte")
                && (value_type.starts_with("int") || value_type == "isize" || value_type == "byte")
            {
                return true;
            }
            if (param_type.starts_with("uint") || param_type == "usize")
                && (value_type.starts_with("uint") || value_type == "usize")
            {
                return true;
            }
            if param_type.starts_with("float") && value_type.starts_with("float") {
                return true;
            }
            // If param is an array of the same generic
            if param_type.starts_with("array<") {
                return true; // array params in handles accept any array
            }
            return false;
        }
        // If no handle signature is found, accept leniently
        true
    }

    /// Monomorphize/specialize the blueprint with concrete generic type arguments
    pub fn specialize(&self, type_args: &[BaseType]) -> BlueprintData {
        let mut map = HashMap::new();
        for (g_param, g_arg) in self.generics.iter().zip(type_args.iter()) {
            map.insert(g_param.clone(), g_arg.clone());
        }
        if map.is_empty() {
            return self.clone();
        }

        let mut new_fields = HashMap::new();
        for (f_name, f_type) in &self.fields {
            new_fields.insert(f_name.clone(), f_type.substitute_generics(&map));
        }

        let mut new_methods = HashMap::new();
        for (m_name, sig) in &self.methods {
            let new_params = sig
                .params
                .iter()
                .map(|p| Param {
                    name: p.name.clone(),
                    type_node: p.type_node.substitute_generics(&map),
                    default_value: p.default_value.clone(),
                    is_variadic: p.is_variadic,
                })
                .collect();
            let new_ret = sig.return_type.substitute_generics(&map);
            new_methods.insert(
                m_name.clone(),
                FnSignature {
                    name: sig.name.clone(),
                    params: new_params,
                    return_type: new_ret,
                    is_virtual: sig.is_virtual,
                    is_abstract: sig.is_abstract,
                    generics: sig.generics.clone(),
                },
            );
        }

        let mut new_handle_signatures = HashMap::new();
        for (h_name, sig) in &self.handle_signatures {
            let new_params = sig
                .params
                .iter()
                .map(|p| Param {
                    name: p.name.clone(),
                    type_node: p.type_node.substitute_generics(&map),
                    default_value: p.default_value.clone(),
                    is_variadic: p.is_variadic,
                })
                .collect();
            let new_ret = sig.return_type.substitute_generics(&map);
            new_handle_signatures.insert(
                h_name.clone(),
                FnSignature {
                    name: sig.name.clone(),
                    params: new_params,
                    return_type: new_ret,
                    is_virtual: sig.is_virtual,
                    is_abstract: sig.is_abstract,
                    generics: sig.generics.clone(),
                },
            );
        }

        BlueprintData {
            name: self.name.clone(),
            fields: new_fields,
            methods: new_methods,
            handles: self.handles.clone(),
            handle_signatures: new_handle_signatures,
            settings: self.settings.clone(),
            generics: Vec::new(),
            params: self.params.clone(),
            variants: self.variants.clone(),
            is_class: self.is_class,
            private_fields: self.private_fields.clone(),
            private_methods: self.private_methods.clone(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SymbolKind — describes the type of symbol in the scope
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub enum SymbolKind {
    /// Standard variable (int, float, bool, custom, etc.)
    Variable {
        type_node: BaseType,
        editability: Editability,
        is_array: bool,
    },
    /// Function (fn)
    Function {
        generics: Vec<BaseType>,
        params: Vec<Param>,
        return_type: BaseType,
        body: Option<Vec<Stmt>>,
    },
    /// Macro (macro)
    Macro {
        params: Vec<Param>,
        return_type: BaseType,
        body: Vec<Stmt>,
    },
    /// blueprint definition (class, struct, custom, enum)
    Blueprint,
    /// label
    Label,
}

// ─────────────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: SymbolKind,
    pub visibility: Visibility,
    pub dependencies: Vec<String>,
    pub is_used: bool,
    pub is_param: bool,
    pub is_uninitialized: bool,
    pub is_compilable: bool,
}

impl Default for SymbolInfo {
    fn default() -> Self {
        SymbolInfo {
            name: String::new(),
            kind: SymbolKind::Variable {
                type_node: BaseType::Unknown,
                editability: Editability::Editable,
                is_array: false,
            },
            visibility: Visibility::Private,
            dependencies: vec![],
            is_used: false,
            is_param: false,
            is_uninitialized: false,
            is_compilable: false,
        }
    }
}

impl SymbolInfo {
    /// Retrieve variable type name as string for compatibility checks
    pub fn type_str(&self) -> String {
        match &self.kind {
            SymbolKind::Variable {
                type_node,
                is_array,
                ..
            } => {
                let base = type_node.as_str();
                if *is_array {
                    format!("array<{}>", base)
                } else {
                    base
                }
            }
            SymbolKind::Function {
                params,
                return_type,
                ..
            } => {
                let p_strs: Vec<String> = params.iter().map(|p| p.type_node.as_str()).collect();
                format!("Fn<({}), {}>", p_strs.join(", "), return_type.as_str())
            }
            SymbolKind::Macro {
                params,
                return_type,
                ..
            } => {
                let p_strs: Vec<String> = params.iter().map(|p| p.type_node.as_str()).collect();
                format!("Macro<({}), {}>", p_strs.join(", "), return_type.as_str())
            }
            SymbolKind::Blueprint => "blueprint".to_string(),
            SymbolKind::Label => "label".to_string(),
        }
    }

    pub fn is_editable(&self) -> bool {
        match &self.kind {
            SymbolKind::Variable { editability, .. } => *editability == Editability::Editable,
            _ => false,
        }
    }

    pub fn is_array(&self) -> bool {
        matches!(&self.kind, SymbolKind::Variable { is_array, .. } if *is_array)
    }

    // ── Compatibility Helpers ──────────────────────────────────────────
    pub fn type_node(&self) -> Option<&BaseType> {
        match &self.kind {
            SymbolKind::Variable { type_node, .. } => Some(type_node),
            SymbolKind::Function { return_type, .. } => Some(return_type),
            SymbolKind::Macro { return_type, .. } => Some(return_type),
            _ => None,
        }
    }

    pub fn editability(&self) -> Option<&Editability> {
        match &self.kind {
            SymbolKind::Variable { editability, .. } => Some(editability),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Environment — current scope with parent chain for lexical scoping
// ─────────────────────────────────────────────────────────────────────────────
pub struct Environment {
    pub parent: Option<Rc<RefCell<Environment>>>,
    /// symbols: variables, functions, blueprints
    pub symbols: HashMap<String, SymbolInfo>,
    /// blueprints: full data for type checking (fields, methods, handles)
    /// Separate from symbols to distinguish between type definitions and instances
    pub blueprints: HashMap<String, BlueprintData>,
    /// child namespaces: maps namespace name to child environment
    pub namespaces: HashMap<String, Rc<RefCell<Environment>>>,
}

impl Environment {
    pub fn new() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Environment {
            parent: None,
            symbols: HashMap::new(),
            blueprints: HashMap::new(),
            namespaces: HashMap::new(),
        }))
    }

    pub fn with_parent(parent: Rc<RefCell<Environment>>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Environment {
            parent: Some(parent),
            symbols: HashMap::new(),
            blueprints: HashMap::new(),
            namespaces: HashMap::new(),
        }))
    }

    pub fn define_namespace(&mut self, name: String, env: Rc<RefCell<Environment>>) {
        self.namespaces.insert(name, env);
    }

    pub fn lookup_namespace(&self, name: &str) -> Option<Rc<RefCell<Environment>>> {
        if let Some(env) = self.namespaces.get(name) {
            return Some(Rc::clone(env));
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow().lookup_namespace(name);
        }
        None
    }

    pub fn get_or_create_namespace(
        &mut self,
        name: &str,
        parent_rc: Rc<RefCell<Environment>>,
    ) -> Rc<RefCell<Environment>> {
        if let Some(env) = self.namespaces.get(name) {
            return Rc::clone(env);
        }
        let new_env = Environment::with_parent(parent_rc);
        self.namespaces.insert(name.to_string(), Rc::clone(&new_env));
        new_env
    }

    pub fn lookup_qualified(&self, namespace_path: &[&str], symbol_name: &str) -> Option<SymbolInfo> {
        if namespace_path.is_empty() {
            return self.lookup(symbol_name);
        }
        let first = namespace_path[0];
        let mut cur = self.lookup_namespace(first)?;
        for &segment in &namespace_path[1..] {
            let next = {
                let borrowed = cur.borrow();
                borrowed.lookup_namespace(segment)?
            };
            cur = next;
        }
        let res = cur.borrow().lookup(symbol_name);
        res
    }

    pub fn lookup_blueprint_qualified(&self, namespace_path: &[&str], bp_name: &str) -> Option<BlueprintData> {
        if namespace_path.is_empty() {
            return self.lookup_blueprint(bp_name);
        }
        let first = namespace_path[0];
        let mut cur = self.lookup_namespace(first)?;
        for &segment in &namespace_path[1..] {
            let next = {
                let borrowed = cur.borrow();
                borrowed.lookup_namespace(segment)?
            };
            cur = next;
        }
        let res = cur.borrow().lookup_blueprint(bp_name);
        res
    }

    // ── Symbol operations ─────────────────────────────────────────────────

    pub fn define(&mut self, name: String, info: SymbolInfo) -> Result<(), String> {
        if let Some(existing) = self.symbols.get(&name) {
            // Functions may always overload each other.
            let both_fns = matches!(existing.kind, SymbolKind::Function { .. })
                && matches!(info.kind, SymbolKind::Function { .. });
            // Macros may always overload each other (different patterns/arities).
            let both_macros = matches!(existing.kind, SymbolKind::Macro { .. })
                && matches!(info.kind, SymbolKind::Macro { .. });
            // Blueprints may be updated (e.g. via `impl`).
            let both_blueprints = matches!(existing.kind, SymbolKind::Blueprint)
                && matches!(info.kind, SymbolKind::Blueprint);

            if both_fns || both_macros || both_blueprints {
                self.symbols.insert(name, info);
                return Ok(());
            }
            return Err(format!(
                "Semantic Error: '{}' is already defined in this scope.",
                name
            ));
        }
        self.symbols.insert(name, info);
        Ok(())
    }

    /// Define or update symbol (used for stdlib injection)
    pub fn define_or_update(&mut self, name: String, info: SymbolInfo) {
        self.symbols.insert(name, info);
    }

    pub fn lookup(&self, name: &str) -> Option<SymbolInfo> {
        if let Some(info) = self.symbols.get(name) {
            return Some(info.clone());
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow().lookup(name);
        }
        None
    }

    pub fn update(&mut self, name: &str, info: SymbolInfo) -> bool {
        if self.symbols.contains_key(name) {
            self.symbols.insert(name.to_string(), info);
            return true;
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow_mut().update(name, info);
        }
        false
    }

    pub fn mark_used(&mut self, name: &str) -> bool {
        if let Some(info) = self.symbols.get_mut(name) {
            info.is_used = true;
            return true;
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow_mut().mark_used(name);
        }
        false
    }

    pub fn mark_initialized(&mut self, name: &str) -> bool {
        if let Some(info) = self.symbols.get_mut(name) {
            info.is_uninitialized = false;
            return true;
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow_mut().mark_initialized(name);
        }
        false
    }

    // ── Blueprint operations ──────────────────────────────────────────────

    pub fn define_blueprint(&mut self, name: String, data: BlueprintData) {
        self.blueprints.insert(name, data);
    }

    pub fn mutate_blueprint<F, R>(&mut self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&mut BlueprintData) -> R,
    {
        let clean = Self::extract_blueprint_name(name).unwrap_or(name);
        let base = clean.split('<').next().unwrap_or(clean).trim();
        if let Some(bp) = self.blueprints.get_mut(base) {
            return Some(f(bp));
        }
        if let Some(bp) = self.blueprints.get_mut(name) {
            return Some(f(bp));
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow_mut().mutate_blueprint(name, f);
        }
        None
    }

    pub fn resolve_type_alias(&self, name: &str) -> Option<BaseType> {
        if let Some(info) = self.symbols.get(name) {
            if let SymbolKind::Variable {
                type_node: BaseType::Type(inner),
                ..
            } = &info.kind {
                    let inner_base = inner.as_ref();
                    if let BaseType::Blueprint {
                        name: next_name, ..
                    }
                    | BaseType::Struct {
                        name: next_name, ..
                    }
                    | BaseType::Class {
                        name: next_name, ..
                    }
                    | BaseType::Enum {
                        name: next_name, ..
                    } = inner_base
                    {
                        if next_name != name {
                            if let Some(deeper) = self.resolve_type_alias(next_name) {
                                return Some(deeper);
                            }
                        }
                    }
                    return Some(inner_base.clone());
            }
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow().resolve_type_alias(name);
        }
        None
    }

    /// Look up blueprint in current and parent scopes
    pub fn lookup_blueprint(&self, name: &str) -> Option<BlueprintData> {
        let clean = Self::extract_blueprint_name(name).unwrap_or(name);
        let base = clean.split('<').next().unwrap_or(clean).trim();
        if let Some(bp) = self.blueprints.get(base) {
            return Some(bp.clone());
        }
        if let Some(bp) = self.blueprints.get(name) {
            return Some(bp.clone());
        }
        if base.contains("::") {
            let parts: Vec<&str> = base.split("::").collect();
            if parts.len() >= 2 {
                if let Some(ns_env) = self.lookup_namespace(parts[0]) {
                    let sub_name = parts[1..].join("::");
                    if let Some(bp) = ns_env.borrow().lookup_blueprint(&sub_name) {
                        return Some(bp);
                    }
                }
            }
        }
        if let Some(aliased) = self.resolve_type_alias(base).or_else(|| self.resolve_type_alias(name)) {
            let target_name = match &aliased {
                BaseType::Blueprint { name, .. }
                | BaseType::Struct { name, .. }
                | BaseType::Class { name, .. }
                | BaseType::Enum { name, .. } => name.clone(),
                BaseType::Array { .. } => "array".to_string(),
                _ => aliased.get_name(),
            };
            if !target_name.is_empty() && target_name != name && target_name != base {
                return self.lookup_blueprint(&target_name);
            }
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow().lookup_blueprint(name);
        }
        None
    }

    /// Look up enum name containing a specific variant
    pub fn lookup_enum_for_variant(&self, variant_name: &str) -> Option<String> {
        for (name, bp) in &self.blueprints {
            if bp.variants.iter().any(|v| v.name == variant_name) {
                return Some(name.clone());
            }
        }
        if let Some(ref parent) = self.parent {
            return parent.borrow().lookup_enum_for_variant(variant_name);
        }
        None
    }

    /// Extract blueprint name from generic types like "class::Node" -> "Node"
    pub fn extract_blueprint_name(type_str: &str) -> Option<&str> {
        for prefix in &["class::", "struct::", "enum::", "blueprint::"] {
            if type_str.starts_with(prefix) {
                return Some(type_str.trim_start_matches(prefix));
            }
        }
        // Direct type name match (e.g. "list")
        None
    }
}

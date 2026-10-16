pub use crate::backend::cpp::runtime::generate_runtime_header;
use crate::backend::cpp::stmt::{cpp_safe_name, is_raw_pointer, type_to_cpp};
use crate::frontend::parser::ast::*;
use crate::middle_end::semantic::analyzer::detect_execution_mode;
use std::collections::{HashMap, HashSet};
use std::mem::{replace, take};
pub struct CodeGenerator {
    pub(crate) output: String,
    pub(crate) indent_level: usize,
    pub(crate) machine_types: HashSet<String>,
    pub(crate) enum_types: HashSet<String>,
    pub(crate) payload_enum_types: HashSet<String>,
    pub(crate) simple_enum_types: HashSet<String>,
    pub(crate) pointer_vars: HashSet<String>,
    pub(crate) raw_pointer_vars: HashSet<String>,
    pub(crate) address_vars: HashSet<String>,
    pub(crate) vars: HashMap<String, String>,
    pub(crate) function_handles: HashMap<String, Vec<Decl>>,
    pub(crate) struct_field_counts: HashMap<String, usize>,
    pub(crate) struct_field_types: HashMap<String, HashMap<String, String>>,
    pub(crate) current_class_name: Option<String>,
    pub(crate) current_fn_name: Option<String>,
    pub(crate) fn_return_types: HashMap<String, String>,
    pub(crate) primitive_impl_methods: HashMap<String, Vec<String>>,
    pub(crate) in_primitive_impl: bool,
    pub(crate) current_primitive_target: Option<String>,
    pub(crate) yield_counter: usize,
    pub(crate) in_class_or_scope: bool,
    pub(crate) in_machine: bool,
    pub(crate) current_block_vars: Option<HashSet<String>>,
    pub(crate) variadic_packs: HashSet<String>,
    pub(crate) target_impl_methods: HashMap<String, Vec<Decl>>,
    pub(crate) target_handle_methods: HashMap<String, Vec<Decl>>,
    pub(crate) c_includes: HashSet<String>,
    pub(crate) iter_counter: usize,
    pub(crate) property_access_types: HashSet<String>,
    pub(crate) type_own_members: HashMap<String, HashSet<String>>,
    pub(crate) types_with_drop: HashSet<String>,
    pub(crate) used_enums: HashSet<String>,
    pub(crate) namespaces: HashSet<String>,
    pub(crate) forward_declared_fns: HashSet<String>,
    pub(crate) type_aliases: HashMap<String, String>,
}

impl CodeGenerator {
    pub fn new() -> Self {
        CodeGenerator {
            output: String::new(),
            indent_level: 0,
            machine_types: HashSet::new(),
            enum_types: HashSet::new(),
            used_enums: HashSet::from(["Option".to_string(), "Result".to_string()]),
            payload_enum_types: HashSet::new(),
            simple_enum_types: HashSet::new(),
            pointer_vars: HashSet::new(),
            raw_pointer_vars: HashSet::new(),
            address_vars: HashSet::new(),
            vars: HashMap::new(),
            function_handles: HashMap::new(),
            struct_field_counts: HashMap::new(),
            struct_field_types: HashMap::new(),
            current_class_name: None,
            current_fn_name: None,
            fn_return_types: HashMap::new(),
            primitive_impl_methods: HashMap::new(),
            in_primitive_impl: false,
            current_primitive_target: None,
            yield_counter: 0,
            in_class_or_scope: false,
            in_machine: false,
            current_block_vars: None,
            variadic_packs: HashSet::new(),
            target_impl_methods: HashMap::new(),
            target_handle_methods: HashMap::new(),
            c_includes: HashSet::new(),
            iter_counter: 0,
            property_access_types: HashSet::new(),
            type_own_members: HashMap::new(),
            types_with_drop: HashSet::new(),
            namespaces: HashSet::new(),
            forward_declared_fns: HashSet::new(),
            type_aliases: HashMap::new(),
        }
    }

    pub(crate) fn resolve_alias(&self, ty: &str) -> String {
        let clean = ty.trim_start_matches("fast_std::");
        let mut cur = clean.to_string();
        for _ in 0..10 {
            if let Some(target) = self.type_aliases.get(&cur) {
                cur = target.trim_start_matches("fast_std::").to_string();
            } else {
                break;
            }
        }
        cur
    }
    pub(crate) fn emit_operator_overloads(&mut self, handle_block: &Option<Vec<Decl>>) {
        if let Some(handles) = handle_block {
            for h in handles {
                if let Decl::FnDecl {
                    name,
                    generics,
                    params,
                    return_type,
                    ..
                } = h
                {
                    let op = match name.as_str() {
                        "add" => Some("+"),
                        "sub" => Some("-"),
                        "mul" => Some("*"),
                        "div" => Some("/"),
                        "mod" => Some("%"),
                        "partial_equal" => Some("=="),
                        "not_equal" => Some("!="),
                        "greater_than" => Some(">"),
                        "less_than" => Some("<"),
                        "greater_than_equal" => Some(">="),
                        "less_than_equal" => Some("<="),
                        "equal" => Some("="),
                        "add_reassign" => Some("+="),
                        "sub_reassign" => Some("-="),
                        "mul_reassign" => Some("*="),
                        "div_reassign" => Some("/="),
                        "mod_reassign" => Some("%="),
                        _ => None,
                    };

                    let ret_str = type_to_cpp(return_type);

                    if let Some(o) = op {
                        if params.len() == 1 {
                            let param_type = type_to_cpp(&params[0].type_node);
                            let param_name = &params[0].name;
                            let (op_ret, is_void) = if ret_str == "void" {
                                (
                                    format!(
                                        "{}&",
                                        self.current_class_name.as_deref().unwrap_or("auto")
                                    ),
                                    true,
                                )
                            } else {
                                (ret_str.clone(), false)
                            };
                            self.emit(&format!(
                                "{} operator{}({} {}) {{",
                                op_ret, o, param_type, param_name
                            ));
                            self.indent_level += 1;
                            if is_void {
                                self.emit(&format!("this->{}({});", name, param_name));
                                self.emit("return *this;");
                            } else {
                                self.emit(&format!("return this->{}({});", name, param_name));
                            }
                            self.indent_level -= 1;
                            self.emit("}");
                        }
                    } else if name == "index_access" {
                        if params.len() == 1 {
                            let param_type = type_to_cpp(&params[0].type_node);
                            let param_name = &params[0].name;
                            self.emit(&format!(
                                "{} operator[]({} {}) {{",
                                ret_str, param_type, param_name
                            ));
                            self.indent_level += 1;
                            self.emit(&format!("return this->index_access({});", param_name));
                            self.indent_level -= 1;
                            self.emit("}");
                        }
                    } else if name == "call" {
                        let has_variadic = params.iter().any(|p| p.is_variadic);
                        if !generics.is_empty() || has_variadic {
                            self.emit("template <typename... __Args>");
                            self.emit("auto operator()(__Args&&... args) -> decltype(this->call(std::forward<__Args>(args)...)) {");
                            self.indent_level += 1;
                            self.emit("return this->call(std::forward<__Args>(args)...);");
                            self.indent_level -= 1;
                            self.emit("}");
                        } else {
                            let param_list: Vec<String> = params
                                .iter()
                                .map(|p| {
                                    let p_type = type_to_cpp(&p.type_node);
                                    format!("{} {}", p_type, p.name)
                                })
                                .collect();
                            let arg_names: Vec<String> =
                                params.iter().map(|p| p.name.clone()).collect();
                            self.emit(&format!(
                                "{} operator()({}) {{",
                                ret_str,
                                param_list.join(", ")
                            ));
                            self.indent_level += 1;
                            if return_type == &BaseType::Void {
                                self.emit(&format!("this->call({});", arg_names.join(", ")));
                            } else {
                                self.emit(&format!("return this->call({});", arg_names.join(", ")));
                            }
                            self.indent_level -= 1;
                            self.emit("}");
                        }
                    } else if name == "cast" {
                        if !generics.is_empty() {
                            let g_name = generics[0].as_str();
                            self.emit(&format!("template <typename {g_name}>"));
                            self.emit(&format!("explicit operator {g_name}() const {{"));
                            self.indent_level += 1;
                            self.emit(&format!(
                                "return const_cast<std::decay_t<decltype(*this)>*>(this)->template cast<{g_name}>();"
                            ));
                            self.indent_level -= 1;
                            self.emit("}");
                        } else if params.is_empty() {
                            self.emit(&format!("explicit operator {}() const {{", ret_str));
                            self.indent_level += 1;
                            self.emit(
                                "return const_cast<std::decay_t<decltype(*this)>*>(this)->cast();",
                            );
                            self.indent_level -= 1;
                            self.emit("}");
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn clean_type_name(ty: &str) -> String {
        let s = ty.trim();
        let s = s.trim_start_matches("const ").trim();
        let s = s.trim_end_matches('*').trim_end_matches('&').trim();
        let s = s.strip_prefix("Option<").unwrap_or(s);
        let s = s.split('<').next().unwrap_or(s).trim();
        let s = s.split("::").last().unwrap_or(s).trim();
        s.to_string()
    }

    pub(crate) fn match_primitive_target(
        &mut self,
        expr: &Expr,
        method_name: &str,
    ) -> Option<String> {
        if let Some(targets) = self.primitive_impl_methods.get(method_name).cloned() {
            let cpp_type = self.extract_cpp_type_from_expr(expr).unwrap_or_default();
            let is_this = matches!(expr, Expr::This);

            let resolved_type = self.resolve_alias(&cpp_type);

            if is_this && self.in_primitive_impl {
                self.current_primitive_target.clone()
            } else if targets
                .iter()
                .any(|t| t == &cpp_type || t == &resolved_type)
            {
                Some(cpp_type)
            } else if targets.contains(&"array".to_string())
                && (resolved_type == "array"
                    || resolved_type.starts_with("fast_std::array")
                    || resolved_type.starts_with("array<")
                    || resolved_type.ends_with("[]")
                    || resolved_type == "str"
                    || resolved_type == "ustr"
                    || resolved_type == "char*"
                    || resolved_type == "char32_t*")
            {
                Some("array".to_string())
            } else if targets.contains(&"char".to_string()) && cpp_type == "char" {
                Some("char".to_string())
            } else if targets.contains(&"bool".to_string())
                && (cpp_type == "bool" || cpp_type == "flag")
            {
                Some("bool".to_string())
            } else {
                None
            }
        } else {
            None
        }
    }

    pub(crate) fn find_assign_handle(
        &self,
        type_name: &str,
        candidates: &[&str],
    ) -> Option<String> {
        let base = Self::clean_type_name(type_name);
        if let Some(handles) = self.target_handle_methods.get(&base) {
            for cand in candidates {
                if handles.iter().any(|h| {
                    if let Decl::FnDecl { name, .. } = h {
                        name == *cand
                    } else {
                        false
                    }
                }) {
                    return Some(cand.to_string());
                }
            }
        }
        for cand in candidates {
            if let Some(targets) = self.primitive_impl_methods.get(*cand) {
                if targets.contains(&base) {
                    return Some(cand.to_string());
                }
            }
        }
        None
    }

    pub(crate) fn emit(&mut self, s: &str) {
        let indent = "    ".repeat(self.indent_level);
        self.output.push_str(&format!("{}{}\n", indent, s));
    }

    pub(crate) fn emit_headers(&mut self) {
        self.output.push_str("#include <clib/fast_runtime.h>\n\n");
    }

    pub(crate) fn is_known_namespace(&self, ns: &str) -> bool {
        self.namespaces.contains(ns)
            || (!ns.is_empty()
                && ns.split("::").all(|part| {
                    part == "std" || part == "fast_std" || self.namespaces.contains(part)
                }))
    }

    pub(crate) fn capture_output<F>(&mut self, f: F) -> String
    where
        F: FnOnce(&mut Self),
    {
        let prev_output = take(&mut self.output);
        f(self);
        replace(&mut self.output, prev_output)
    }

    pub fn scan_declarations(&mut self, flat_ast: &[Stmt]) {
        for stmt in flat_ast {
            if let Stmt::Declaration(Decl::BlueprintDecl {
                name,
                definition,
                share_directive,
                ..
            }) = stmt
            {
                if matches!(name.as_str(), "array" | "type") {
                    continue;
                }
                let clean_name = name.split('<').next().unwrap_or(name).trim().to_string();
                if let BlueprintDef::Explicit(fields) = definition {
                    self.struct_field_counts.insert(name.clone(), fields.len());
                    let mut fields_map = HashMap::new();
                    for f in fields {
                        fields_map.insert(f.name.clone(), type_to_cpp(&f.type_node));
                        self.type_own_members
                            .entry(clean_name.clone())
                            .or_insert_with(HashSet::new)
                            .insert(f.name.clone());
                    }
                    self.struct_field_types
                        .insert(clean_name.clone(), fields_map.clone());
                    self.struct_field_types.insert(name.clone(), fields_map);
                }
                if let Some(sd) = share_directive {
                    self.type_own_members
                        .entry(clean_name.clone())
                        .or_insert_with(HashSet::new)
                        .insert(sd.target_field.clone());
                }
            } else if let Stmt::Declaration(Decl::StructDecl {
                name,
                public_block,
                private_block,
                handle_block,
                ..
            }) = stmt
            {
                let cnt = public_block
                    .iter()
                    .chain(private_block.iter())
                    .filter(|d| matches!(d, Decl::VarDecl { .. }))
                    .count();
                self.struct_field_counts.insert(name.clone(), cnt);
                self.target_handle_methods
                    .entry(name.clone())
                    .or_insert_with(Vec::new)
                    .extend(handle_block.clone());
                let mut fields_map = HashMap::new();
                for d in public_block.iter().chain(private_block.iter()) {
                    if let Decl::VarDecl {
                        name: f_name,
                        type_node,
                        ..
                    } = d
                    {
                        fields_map.insert(f_name.clone(), type_to_cpp(type_node));
                    }
                }
                let clean_name = if let Some(idx) = name.find('<') {
                    name[..idx].to_string()
                } else {
                    name.clone()
                };
                self.struct_field_types
                    .insert(clean_name, fields_map.clone());
                self.struct_field_types.insert(name.clone(), fields_map);
            } else if let Stmt::Declaration(Decl::ClassDecl {
                name,
                public_block,
                private_block,
                handle_block,
                ..
            }) = stmt
            {
                let cnt = public_block
                    .iter()
                    .chain(private_block.iter())
                    .filter(|d| matches!(d, Decl::VarDecl { .. }))
                    .count();
                self.struct_field_counts.insert(name.clone(), cnt);
                self.target_handle_methods
                    .entry(name.clone())
                    .or_insert_with(Vec::new)
                    .extend(handle_block.clone());
                let mut fields_map = HashMap::new();
                for d in public_block.iter().chain(private_block.iter()) {
                    if let Decl::VarDecl {
                        name: f_name,
                        type_node,
                        ..
                    } = d
                    {
                        fields_map.insert(f_name.clone(), type_to_cpp(type_node));
                    }
                }
                let clean_name = if let Some(idx) = name.find('<') {
                    name[..idx].to_string()
                } else {
                    name.clone()
                };
                self.struct_field_types
                    .insert(clean_name, fields_map.clone());
                self.struct_field_types.insert(name.clone(), fields_map);
            } else if let Stmt::Declaration(Decl::EnumDecl {
                name, handle_block, ..
            }) = stmt
            {
                self.target_handle_methods
                    .entry(name.clone())
                    .or_insert_with(Vec::new)
                    .extend(handle_block.clone());
            } else if let Stmt::Declaration(Decl::MachineDecl {
                name, handle_block, ..
            }) = stmt
            {
                self.target_handle_methods
                    .entry(name.clone())
                    .or_insert_with(Vec::new)
                    .extend(handle_block.clone());
            } else if let Stmt::Declaration(Decl::ImplDecl {
                target,
                is_handle_impl,
                methods,
                handle_block,
                ..
            }) = stmt
            {
                if target == "type" {
                    continue;
                }
                let is_primitive = matches!(
                    target.as_str(),
                    "char" | "bool" | "flag" | "array" | "byte" | "usize" | "isize"
                ) || target.starts_with("int")
                    || target.starts_with("uint")
                    || target.starts_with("float")
                    || target.starts_with("array");
                if is_primitive {
                    let prim_target = if target.starts_with("array") {
                        "array".to_string()
                    } else {
                        target.clone()
                    };
                    for m in methods {
                        if let Decl::FnDecl { name, .. } = m {
                            let list = self
                                .primitive_impl_methods
                                .entry(name.clone())
                                .or_insert_with(Vec::new);
                            if !list.contains(&prim_target) {
                                list.push(prim_target.clone());
                            }
                            if !list.contains(target) {
                                list.push(target.clone());
                            }
                        }
                    }
                    for h in handle_block {
                        if let Decl::FnDecl { name, .. } = h {
                            let list = self
                                .primitive_impl_methods
                                .entry(name.clone())
                                .or_insert_with(Vec::new);
                            if !list.contains(&prim_target) {
                                list.push(prim_target.clone());
                            }
                            if !list.contains(target) {
                                list.push(target.clone());
                            }
                        }
                    }
                } else {
                    let clean_target = target
                        .split('<')
                        .next()
                        .unwrap_or(target)
                        .trim()
                        .to_string();
                    for h in handle_block {
                        if let Decl::FnDecl { name: fn_name, .. } = h {
                            if fn_name == "property_access"
                                || fn_name == "handle_access"
                                || fn_name == "namespace_access"
                                || fn_name == "arrow"
                                || fn_name == "fat_arrow"
                            {
                                self.property_access_types.insert(clean_target.clone());
                            }
                            self.type_own_members
                                .entry(clean_target.clone())
                                .or_insert_with(HashSet::new)
                                .insert(fn_name.clone());
                        }
                    }
                    for m in methods {
                        if let Decl::FnDecl { name: fn_name, .. } = m {
                            self.type_own_members
                                .entry(clean_target.clone())
                                .or_insert_with(HashSet::new)
                                .insert(fn_name.clone());
                        }
                    }
                    self.function_handles
                        .entry(target.clone())
                        .or_insert_with(Vec::new)
                        .extend(handle_block.clone());
                    self.target_handle_methods
                        .entry(target.clone())
                        .or_insert_with(Vec::new)
                        .extend(handle_block.clone());
                    if !*is_handle_impl {
                        self.target_impl_methods
                            .entry(target.clone())
                            .or_insert_with(Vec::new)
                            .extend(methods.clone());
                    }
                }
            } else if let Stmt::Declaration(Decl::NamespaceDecl { name, decls, .. }) = stmt {
                self.namespaces.insert(name.clone());
                fn collect_inner_namespaces(
                    decls: &[Decl],
                    parent: &str,
                    namespaces: &mut HashSet<String>,
                ) {
                    for d in decls {
                        if let Decl::NamespaceDecl { name, decls, .. } = d {
                            let full = format!("{}::{}", parent, name);
                            namespaces.insert(name.clone());
                            namespaces.insert(full.clone());
                            collect_inner_namespaces(decls, &full, namespaces);
                        }
                    }
                }
                collect_inner_namespaces(decls, name, &mut self.namespaces);
            } else if let Stmt::Declaration(Decl::BlockDecl {
                visibility, name, ..
            }) = stmt
            {
                if *visibility == Visibility::Public {
                    self.namespaces.insert(name.clone());
                }
            } else if let Stmt::Declaration(Decl::DefineDecl {
                name,
                type_alias: Some(t),
                ..
            }) = stmt
            {
                let cpp_t = type_to_cpp(t);
                self.type_aliases.insert(name.clone(), cpp_t);
            } else if let Stmt::Declaration(Decl::FnDecl {
                name, return_type, ..
            }) = stmt
            {
                let cpp_t = type_to_cpp(return_type);
                self.fn_return_types.insert(name.clone(), cpp_t);
            }
        }
    }

    pub fn generate(&mut self, ast: &Vec<Stmt>, emit_headers: bool, wrap_in_main: bool) -> String {
        if emit_headers {
            self.emit_headers();
        }

        let flat_ast: Vec<Stmt> = ast.to_vec();

        fn collect_usings(stmts: &[Stmt], used: &mut HashSet<String>) {
            for s in stmts {
                match s {
                    Stmt::UsingStmt(name) => {
                        used.insert(name.clone());
                    }
                    Stmt::Block(inner) | Stmt::ThisBlock(inner) => {
                        collect_usings(inner, used);
                    }
                    Stmt::Declaration(Decl::FnDecl { body, .. }) => {
                        collect_usings(body, used);
                    }
                    Stmt::IfStmt {
                        then_block,
                        else_block,
                        ..
                    } => {
                        collect_usings(then_block, used);
                        if let Some(eb) = else_block {
                            collect_usings(eb, used);
                        }
                    }
                    _ => {}
                }
            }
        }
        collect_usings(&flat_ast, &mut self.used_enums);

        self.scan_declarations(&flat_ast);

        // Pre-pass for Blueprints and Impls
        let mut blueprints: HashMap<
            String,
            (Vec<BaseType>, BlueprintDef, Option<BlueprintShareDirective>),
        > = HashMap::new();
        let mut impls: HashMap<String, Vec<Decl>> = HashMap::new();
        let mut blueprint_handles: HashMap<String, Vec<Decl>> = HashMap::new();
        let mut primitive_impls: Vec<(String, Vec<BaseType>, Vec<Decl>, Vec<Decl>)> = Vec::new();

        for stmt in &flat_ast {
            if let Stmt::Declaration(Decl::BlueprintDecl {
                name,
                generics,
                definition,
                share_directive,
                ..
            }) = stmt
            {
                if matches!(name.as_str(), "array" | "type") {
                    continue;
                }
                blueprints.insert(
                    name.clone(),
                    (
                        generics.clone(),
                        definition.clone(),
                        share_directive.clone(),
                    ),
                );
            } else if let Stmt::Declaration(Decl::ImplDecl {
                target,
                target_generics,
                is_handle_impl,
                methods,
                handle_block,
            }) = stmt
            {
                if target == "type" {
                    continue;
                }
                let is_primitive = matches!(
                    target.as_str(),
                    "char" | "bool" | "flag" | "array" | "byte" | "usize" | "isize"
                ) || target.starts_with("int")
                    || target.starts_with("uint")
                    || target.starts_with("float")
                    || target.starts_with("array");
                if is_primitive {
                    primitive_impls.push((
                        target.clone(),
                        target_generics.clone(),
                        methods.clone(),
                        handle_block.clone(),
                    ));
                } else {
                    if *is_handle_impl {
                        blueprint_handles
                            .entry(target.clone())
                            .or_insert_with(Vec::new)
                            .extend(handle_block.clone());
                    } else {
                        impls
                            .entry(target.clone())
                            .or_insert_with(Vec::new)
                            .extend(methods.clone());
                        blueprint_handles
                            .entry(target.clone())
                            .or_insert_with(Vec::new)
                            .extend(handle_block.clone());
                    }
                }
            }
        }

        for stmt in &flat_ast {
            if let Stmt::UsingStmt(name) = stmt {
                if self.is_known_namespace(name) {
                    self.emit(&format!("using namespace {};", name));
                }
            }
        }

        for stmt in &flat_ast {
            match stmt {
                Stmt::Declaration(Decl::ClassDecl { name, generics, .. }) => {
                    if !generics.is_empty() {
                        let tparams: Vec<String> = generics
                            .iter()
                            .map(|g| {
                                let s = g.as_str();
                                if s.starts_with("...") {
                                    let clean = s.trim_start_matches('.');
                                    format!("typename {}, typename... _Rest_{}", clean, clean)
                                } else {
                                    format!("typename {}", s.trim_start_matches('.'))
                                }
                            })
                            .collect();
                        self.emit(&format!(
                            "template <{}> class {};",
                            tparams.join(", "),
                            name
                        ));
                    } else {
                        self.emit(&format!("class {};", name));
                    }
                }
                Stmt::Declaration(Decl::StructDecl { name, .. }) => {
                    self.emit(&format!("struct {};", name));
                }
                _ => {}
            }
        }

        for (name, (generics, _, _)) in &blueprints {
            if !generics.is_empty() {
                let tparams: Vec<String> = generics
                    .iter()
                    .map(|g| {
                        let s = g.as_str();
                        if s.starts_with("...") {
                            let clean = s.trim_start_matches('.');
                            format!("typename {}, typename... _Rest_{}", clean, clean)
                        } else {
                            format!("typename {}", s)
                        }
                    })
                    .collect();
                self.emit(&format!(
                    "template <{}> struct {};",
                    tparams.join(", "),
                    name
                ));
            } else {
                self.emit(&format!("struct {};", name));
            }
        }

        // Emit all type aliases (using <name> = <type>;) before struct/blueprint bodies
        for stmt in &flat_ast {
            if let Stmt::Declaration(Decl::DefineDecl {
                name,
                type_alias: Some(t),
                ..
            }) = stmt
            {
                let cpp_t = type_to_cpp(t);
                self.type_aliases.insert(name.clone(), cpp_t);
                self.visit_statement(stmt);
            }
        }

        let mut blueprint_names: Vec<String> = blueprints.keys().cloned().collect();
        blueprint_names.sort_by(|a, b| {
            let a_gen = !blueprints[a].0.is_empty();
            let b_gen = !blueprints[b].0.is_empty();
            match (a_gen, b_gen) {
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
                _ => a.cmp(b),
            }
        });

        for name in blueprint_names {
            let (generics, definition, share_directive) = blueprints.remove(&name).unwrap();
            match definition {
                BlueprintDef::Explicit(fields) => {
                    if !generics.is_empty() {
                        let tparams: Vec<String> = generics
                            .iter()
                            .map(|g| {
                                let s = g.as_str();
                                if s.starts_with("...") {
                                    let clean = s.trim_start_matches('.');
                                    format!("typename {}, typename... _Rest_{}", clean, clean)
                                } else {
                                    format!("typename {}", s)
                                }
                            })
                            .collect();
                        self.emit(&format!("template <{}>", tparams.join(", ")));
                    }
                    let old_class_name = self.current_class_name.clone();
                    let clean_name = name.split('<').next().unwrap_or(&name).trim().to_string();
                    self.current_class_name = Some(clean_name);
                    self.emit(&format!("struct {} {{", name));
                    self.indent_level += 1;

                    for field in &fields {
                        let cpp_f_type = type_to_cpp(&field.type_node);
                        if is_raw_pointer(&field.type_node, &cpp_f_type) {
                            self.raw_pointer_vars.insert(field.name.clone());
                            self.raw_pointer_vars
                                .insert(format!("this->{}", field.name));
                            self.pointer_vars.insert(field.name.clone());
                            self.pointer_vars.insert(format!("this->{}", field.name));
                        }
                        if matches!(field.type_node, BaseType::Type(_)) {
                            if let Some(ref def_val) = field.default_value {
                                let def_code = self.visit_expression(def_val);
                                self.emit(&format!("type {} = \"{}\";", field.name, def_code));
                            } else {
                                self.emit(&format!("type {} = \"\";", field.name));
                            }
                        } else if let BaseType::Array { ref base_type, .. } = field.type_node {
                            if matches!(&**base_type, BaseType::Type(_)) {
                                self.emit(&format!(
                                    "fast_std::array<type> {} = {{}}; ",
                                    field.name
                                ));
                            } else {
                                let type_str = type_to_cpp(&field.type_node);
                                if let Some(ref def_val) = field.default_value {
                                    let def_code = self.visit_expression(def_val);
                                    self.emit(&format!(
                                        "{} {} = {};",
                                        type_str, field.name, def_code
                                    ));
                                } else {
                                    self.emit(&format!("{} {};", type_str, field.name));
                                }
                            }
                        } else {
                            let type_str = type_to_cpp(&field.type_node);
                            if let Some(ref def_val) = field.default_value {
                                let def_code = self.visit_expression(def_val);
                                self.emit(&format!("{} {} = {};", type_str, field.name, def_code));
                            } else {
                                self.emit(&format!("{} {};", type_str, field.name));
                            }
                        }
                    }

                    if let Some(ref sd) = share_directive {
                        let target_f = &sd.target_field;
                        let cond_str = if let Some(ref c) = sd.condition {
                            let mut cond_code = self.visit_expression(c);
                            cond_code = cond_code.replace("this.", "this->");
                            format!("if ({}) ", cond_code)
                        } else {
                            String::new()
                        };

                        self.emit(&format!("{name}() = default;"));
                        self.emit(&format!("{name}(const {name}& other) : ptr(other.ptr), {target_f}(other.{target_f}) {{"));
                        self.indent_level += 1;
                        if !cond_str.is_empty() {
                            self.emit(&format!("{cond_str}{{"));
                            self.indent_level += 1;
                        }
                        self.emit(&format!(
                            "if (this->{target_f}) {{ this->{target_f}->share(); }}"
                        ));
                        if !cond_str.is_empty() {
                            self.indent_level -= 1;
                            self.emit("}");
                        }
                        self.indent_level -= 1;
                        self.emit("}");

                        self.emit(&format!("{name}& operator=(const {name}& other) {{"));
                        self.indent_level += 1;
                        self.emit("if (this != &other) {");
                        self.indent_level += 1;
                        self.emit("this->ptr = other.ptr;");
                        self.emit(&format!("this->{target_f} = other.{target_f};"));
                        if !cond_str.is_empty() {
                            self.emit(&format!("{cond_str}{{"));
                            self.indent_level += 1;
                        }
                        self.emit(&format!(
                            "if (this->{target_f}) {{ this->{target_f}->share(); }}"
                        ));
                        if !cond_str.is_empty() {
                            self.indent_level -= 1;
                            self.emit("}");
                        }
                        self.indent_level -= 1;
                        self.emit("}");
                        self.emit("return *this;");
                        self.indent_level -= 1;
                        self.emit("}");
                    }

                    let ptr_field = fields.iter().find(|f| f.name == "ptr");
                    if let Some(pf) = ptr_field {
                        let inner_t = match &pf.type_node {
                            BaseType::RawPointer(inner) | BaseType::Address(inner) => {
                                type_to_cpp(inner)
                            }
                            _ => {
                                if !generics.is_empty() {
                                    generics[0].as_str().trim_start_matches('.').to_string()
                                } else {
                                    "void".to_string()
                                }
                            }
                        };
                        let inner_t = inner_t.trim_start_matches('.').to_string();
                        if inner_t != "void" {
                            if let Some(ref sd) = share_directive {
                                let target_f = &sd.target_field;
                                self.emit(&format!("using _ShareTargetT_{target_f} = std::remove_pointer_t<decltype({target_f})>;"));
                                self.emit(&format!("{name}({inner_t}* p) : ptr(p), {target_f}(p ? new _ShareTargetT_{target_f}{{1}} : nullptr) {{}}"));
                                self.emit(&format!("{name}(const {inner_t}* p) : ptr(const_cast<{inner_t}*>(p)), {target_f}(p ? new _ShareTargetT_{target_f}{{1}} : nullptr) {{}}"));
                                self.emit(&format!("{name}({inner_t}& r) : ptr(&r), {target_f}(new _ShareTargetT_{target_f}{{1}}) {{}}"));
                                self.emit(&format!("{name}(const {inner_t}& r) : ptr(const_cast<{inner_t}*>(&r)), {target_f}(new _ShareTargetT_{target_f}{{1}}) {{}}"));
                                self.emit(&format!("{name}& operator=({inner_t}* p) {{ *this = {name}(p); return *this; }}"));
                                self.emit(&format!("{name}& operator=(const {inner_t}* p) {{ *this = {name}(p); return *this; }}"));
                                self.emit(&format!("{name}& operator=({inner_t}& r) {{ *this = {name}(&r); return *this; }}"));
                                self.emit(&format!("{name}& operator=(const {inner_t}& r) {{ *this = {name}(r); return *this; }}"));
                                self.emit(&format!("template <typename _Fn, typename = std::enable_if_t<std::is_function_v<_Fn>>> {name}(_Fn* p) : ptr(new {inner_t}(p)), {target_f}(new _ShareTargetT_{target_f}{{1}}) {{}}"));
                                self.emit(&format!("template <typename _Fn, typename = std::enable_if_t<std::is_function_v<_Fn>>> {name}& operator=(_Fn* p) {{ *this = {name}(p); return *this; }}"));
                            } else {
                                self.emit(&format!("{name}() : ptr(nullptr) {{}}"));
                                self.emit(&format!("{name}({inner_t}* p) : ptr(p) {{}}"));
                                self.emit(&format!(
                                    "{name}(const {inner_t}* p) : ptr(const_cast<{inner_t}*>(p)) {{}}"
                                ));
                                self.emit(&format!("{name}({inner_t}& r) : ptr(&r) {{}}"));
                                self.emit(&format!(
                                    "{name}(const {inner_t}& r) : ptr(const_cast<{inner_t}*>(&r)) {{}}"
                                ));
                                self.emit(&format!(
                                    "{name}& operator=({inner_t}* p) {{ ptr = p; return *this; }}"
                                ));
                                self.emit(&format!(
                                    "{name}& operator=({inner_t}& r) {{ ptr = &r; return *this; }}"
                                ));
                                self.emit(&format!("template <typename _Fn, typename = std::enable_if_t<std::is_function_v<_Fn>>> {name}(_Fn* p) : ptr(new {inner_t}(p)) {{}}"));
                                self.emit(&format!("template <typename _Fn, typename = std::enable_if_t<std::is_function_v<_Fn>>> {name}& operator=(_Fn* p) {{ *this = {name}(p); return *this; }}"));
                            }
                            self.emit(&format!("{inner_t}& operator*() const {{ return *ptr; }}"));
                            self.emit(&format!("{inner_t}* operator->() const {{ return ptr; }}"));
                            self.emit("template <typename... __Args> auto operator()(__Args&&... args) -> decltype((*ptr)(std::forward<__Args>(args)...)) { return (*ptr)(std::forward<__Args>(args)...); }");
                            self.emit(&format!("operator {inner_t}*() const {{ return ptr; }}"));
                            self.emit(&format!("operator {inner_t}&() {{ return *ptr; }}"));
                            self.emit(&format!(
                                "operator const {inner_t}&() const {{ return *ptr; }}"
                            ));
                            self.emit("explicit operator bool() const { return ptr != nullptr; }");
                            self.emit(
                                "bool operator==(std::nullptr_t) const { return ptr == nullptr; }",
                            );
                            self.emit(
                                "bool operator!=(std::nullptr_t) const { return ptr != nullptr; }",
                            );
                        }
                    }

                    if let Some(methods) = impls.get(&name) {
                        for m in methods {
                            if let Decl::FnDecl {
                                name: fn_name,
                                params,
                                ..
                            } = m
                            {
                                if matches!(
                                    fn_name.as_str(),
                                    "property_access"
                                        | "handle_access"
                                        | "namespace_access"
                                        | "arrow"
                                        | "fat_arrow"
                                ) && params.len() == 1
                                    && matches!(
                                        params[0].type_node.as_str().as_str(),
                                        "MemberType" | "blueprint::MemberType" | "unknown"
                                    )
                                {
                                    continue;
                                }
                            }
                            self.visit_declaration(m);
                        }
                    }

                    if let Some(handles) = blueprint_handles.get(&name) {
                        for h in handles {
                            if let Decl::FnDecl {
                                name: fn_name,
                                params,
                                ..
                            } = h
                            {
                                if matches!(
                                    fn_name.as_str(),
                                    "property_access"
                                        | "handle_access"
                                        | "namespace_access"
                                        | "arrow"
                                        | "fat_arrow"
                                ) && params.len() == 1
                                    && matches!(
                                        params[0].type_node.as_str().as_str(),
                                        "MemberType" | "blueprint::MemberType" | "unknown"
                                    )
                                {
                                    continue;
                                }
                            }
                            self.visit_declaration(h);
                            if let Decl::FnDecl {
                                name: fn_name,
                                params,
                                return_type,
                                ..
                            } = h
                            {
                                let op = match fn_name.as_str() {
                                    "add" => Some("+"),
                                    "sub" => Some("-"),
                                    "mul" => Some("*"),
                                    "div" => Some("/"),
                                    "mod" => Some("%"),
                                    "partial_equal" => Some("=="),
                                    "not_equal" => Some("!="),
                                    "less_than" => Some("<"),
                                    "greater_than" => Some(">"),
                                    "less_than_equal" => Some("<="),
                                    "greater_than_equal" => Some(">="),
                                    "equal" | "assign" => Some("="),
                                    "add_reassign" => Some("+="),
                                    "sub_reassign" => Some("-="),
                                    "mul_reassign" => Some("*="),
                                    "div_reassign" => Some("/="),
                                    "mod_reassign" => Some("%="),
                                    _ => None,
                                };
                                let ret_str = type_to_cpp(return_type);
                                if let Some(o) = op {
                                    if params.len() == 1 {
                                        let param_type = type_to_cpp(&params[0].type_node);
                                        let param_name = &params[0].name;
                                        let (op_ret, is_void) = if ret_str == "void" {
                                            (format!("{}&", name), true)
                                        } else {
                                            (ret_str, false)
                                        };
                                        self.emit(&format!(
                                            "{} operator{}({} {}) {{",
                                            op_ret, o, param_type, param_name
                                        ));
                                        self.indent_level += 1;
                                        if is_void {
                                            self.emit(&format!(
                                                "this->{}({});",
                                                fn_name, param_name
                                            ));
                                            self.emit("return *this;");
                                        } else {
                                            self.emit(&format!(
                                                "return this->{}({});",
                                                fn_name, param_name
                                            ));
                                        }
                                        self.indent_level -= 1;
                                        self.emit("}");
                                    }
                                } else if fn_name == "index_access" {
                                    if params.len() == 1 {
                                        let param_type = type_to_cpp(&params[0].type_node);
                                        let param_name = &params[0].name;
                                        self.emit(&format!(
                                            "{} operator[]({} {}) {{",
                                            ret_str, param_type, param_name
                                        ));
                                        self.indent_level += 1;
                                        self.emit(&format!(
                                            "return this->{}({});",
                                            fn_name, param_name
                                        ));
                                        self.indent_level -= 1;
                                        self.emit("}");
                                    }
                                } else if fn_name == "call"
                                    && !fields.iter().any(|f| f.name == "ptr")
                                {
                                    let param_sigs: Vec<String> = params
                                        .iter()
                                        .map(|p| {
                                            let pt = type_to_cpp(&p.type_node);
                                            format!("{} {}", pt, p.name)
                                        })
                                        .collect();
                                    let param_names: Vec<String> =
                                        params.iter().map(|p| p.name.clone()).collect();
                                    self.emit(&format!(
                                        "{} operator()({}) {{",
                                        ret_str,
                                        param_sigs.join(", ")
                                    ));
                                    self.indent_level += 1;
                                    self.emit(&format!(
                                        "return this->{}({});",
                                        fn_name,
                                        param_names.join(", ")
                                    ));
                                    self.indent_level -= 1;
                                    self.emit("}");
                                } else if fn_name == "deref" && params.is_empty() {
                                    self.emit(&format!("{} operator*() {{", ret_str));
                                    self.indent_level += 1;
                                    self.emit(&format!("return this->{}();", fn_name));
                                    self.indent_level -= 1;
                                    self.emit("}");
                                }
                            }
                        }
                    }

                    if let Some(handles) = blueprint_handles.get(&name) {
                        let has_drop_handle = handles.iter().any(|h| {
                            if let Decl::FnDecl { name: fn_name, .. } = h {
                                fn_name == "drop"
                            } else {
                                false
                            }
                        });
                        if has_drop_handle {
                            self.types_with_drop.insert(name.clone());
                            self.emit("bool _fastlang_dropped = false;");
                            self.emit("void _fastlang_call_drop() {");
                            self.emit("    if (!_fastlang_dropped) {");
                            self.emit("        _fastlang_dropped = true;");
                            self.emit("        this->drop();");
                            self.emit("    }");
                            self.emit("}");
                        }
                    }

                    self.indent_level -= 1;
                    self.emit("};");
                    self.current_class_name = old_class_name;
                }
                _ => {
                    self.emit(&format!("// Unsupported BlueprintDef for {}", name));
                }
            }
        }

        for stmt in ast {
            if let Stmt::Declaration(Decl::MachineDecl { name, .. }) = stmt {
                self.machine_types.insert(name.clone());
            }
        }

        // 1. Emit all top-level types (Enums, Structs, Classes, Machines, Arrays) first
        for stmt in ast {
            match &stmt {
                Stmt::Declaration(Decl::EnumDecl { .. })
                | Stmt::Declaration(Decl::ClassDecl { .. })
                | Stmt::Declaration(Decl::StructDecl { .. })
                | Stmt::Declaration(Decl::ArrayDecl { .. })
                | Stmt::Declaration(Decl::MachineDecl { .. }) => {
                    self.visit_statement(stmt);
                }
                _ => {}
            }
        }

        // 2. Emit forward declarations for all top-level functions so forward referencing works seamlessly
        for stmt in ast {
            if let Stmt::Declaration(Decl::FnDecl {
                name,
                generics,
                params,
                return_type,
                body,
                is_abstract,
                ..
            }) = stmt
            {
                if name != "main"
                    && !name.starts_with("@compile::")
                    && !*is_abstract
                    && !body.is_empty()
                    && detect_execution_mode(body) != ExecutionMode::FullyCompilable
                {
                    let has_variadic_param = params.iter().any(|p| p.is_variadic);
                    let mut gen_names: Vec<String> = generics
                        .iter()
                        .map(|g| {
                            let s = g.as_str();
                            if s.starts_with("...") {
                                format!("typename... {}", s.trim_start_matches('.'))
                            } else {
                                format!("typename {}", s)
                            }
                        })
                        .collect();
                    if gen_names.is_empty() && has_variadic_param {
                        gen_names.push("typename... __Args".to_string());
                    }
                    if !gen_names.is_empty() {
                        self.emit(&format!("template <{}>", gen_names.join(", ")));
                    }
                    let ret_type_str = type_to_cpp(return_type);
                    let mut param_strs = Vec::new();
                    for p in params {
                        let cpp_t = type_to_cpp(&p.type_node);
                        if p.is_variadic {
                            let base_pack = if cpp_t.is_empty() || cpp_t == "any" || cpp_t == "auto"
                            {
                                "__Args".to_string()
                            } else {
                                cpp_t
                                    .trim_start_matches('.')
                                    .trim_end_matches('*')
                                    .trim()
                                    .to_string()
                            };
                            param_strs.push(format!("{}... {}", base_pack, p.name));
                        } else if let Some(ref def_val) = p.default_value {
                            let def_code = self.visit_expression(def_val);
                            param_strs.push(format!("{} {} = {}", cpp_t, p.name, def_code));
                        } else {
                            param_strs.push(format!("{} {}", cpp_t, p.name));
                        }
                    }
                    let safe_name = cpp_safe_name(name);
                    self.emit(&format!(
                        "inline {} {}({});",
                        ret_type_str,
                        safe_name,
                        param_strs.join(", ")
                    ));
                    self.forward_declared_fns.insert(name.clone());
                }
            }
        }

        // 3. Generate functions, namespaces, globals, imports
        for stmt in ast {
            match &stmt {
                Stmt::Declaration(Decl::FnDecl { .. })
                | Stmt::Declaration(Decl::ExternFnDecl { .. })
                | Stmt::Declaration(Decl::ExternBlockDecl { .. })
                | Stmt::Declaration(Decl::MicroDecl { .. })
                | Stmt::Declaration(Decl::MacroDecl { .. })
                | Stmt::Declaration(Decl::NamespaceDecl { .. })
                | Stmt::Declaration(Decl::BlockDecl { .. })
                | Stmt::Declaration(Decl::DefineDecl {
                    type_alias: None, ..
                })
                | Stmt::Declaration(Decl::DestructureDecl { .. })
                | Stmt::Declaration(Decl::VarDecl { .. }) => {
                    self.visit_statement(stmt);
                }
                Stmt::Declaration(Decl::Import {
                    module_path,
                    imports,
                    abi,
                    alias,
                }) => {
                    if abi.is_some() {
                        self.visit_statement(stmt);
                    } else if let Some(first) = module_path.first() {
                        if first.ends_with(".h") || first.ends_with(".hpp") {
                            self.visit_statement(stmt);
                        } else {
                            let cpp_namespace = if first == "std" {
                                "fast_std".to_string()
                            } else {
                                module_path.join("_")
                            };
                            if let Some(alias_name) = alias {
                                self.emit(&format!(
                                    "namespace {} = {};",
                                    alias_name, cpp_namespace
                                ));
                            } else if let Some(selected) = imports {
                                for sym in selected {
                                    self.emit(&format!("using {}::{};", cpp_namespace, sym));
                                }
                            } else if cpp_namespace != "fast_std" {
                                self.emit(&format!("using namespace {};", cpp_namespace));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        for (target, target_generics, methods, handle_block) in primitive_impls {
            let clean_target = if target.starts_with("array") {
                "array".to_string()
            } else {
                target.clone()
            };
            let target_base_type = if target == "array" || target.starts_with("array<") {
                if target == "array<char>" {
                    BaseType::Array {
                        base_type: Box::new(BaseType::Char),
                        size: Box::new(None),
                    }
                } else {
                    BaseType::Array {
                        base_type: Box::new(if target_generics.is_empty() {
                            BaseType::GenericParam("T".to_string())
                        } else {
                            target_generics[0].clone()
                        }),
                        size: Box::new(None),
                    }
                }
            } else {
                BaseType::from_str(&target)
            };
            let target_cpp = type_to_cpp(&target_base_type);

            let is_generic = !target_generics.is_empty()
                && target_generics
                    .iter()
                    .any(|g| matches!(g, BaseType::GenericParam(_)));

            for m in methods.iter().chain(handle_block.iter()) {
                if let Decl::FnDecl {
                    name,
                    params,
                    return_type,
                    body,
                    generics,
                    ..
                } = m
                {
                    let ret_cpp = type_to_cpp(return_type);
                    let mut param_cpps = Vec::new();
                    param_cpps.push(format!("{} __this", target_cpp));
                    let mut extra_gen_params = Vec::new();
                    for (idx, p) in params.iter().enumerate() {
                        if matches!(p.type_node, BaseType::Lambda { .. }) {
                            let func_t = format!("__Func{}", idx);
                            extra_gen_params.push(format!("typename {}", func_t));
                            param_cpps.push(format!("{} {}", func_t, p.name));
                        } else {
                            param_cpps.push(format!("{} {}", type_to_cpp(&p.type_node), p.name));
                        }
                    }

                    let mut all_gen_params: Vec<String> = Vec::new();
                    for g in generics {
                        let clean = g.as_str();
                        let clean = clean.trim_start_matches('.');
                        let s = format!("typename {}", clean);
                        if !all_gen_params.contains(&s) {
                            all_gen_params.push(s);
                        }
                    }
                    if is_generic {
                        for g in &target_generics {
                            let s = format!("typename {}", g.as_str());
                            if !all_gen_params.contains(&s) {
                                all_gen_params.push(s);
                            }
                        }
                    }
                    all_gen_params.extend(extra_gen_params);

                    if !all_gen_params.is_empty() {
                        self.emit(&format!("template <{}>", all_gen_params.join(", ")));
                    }
                    self.emit(&format!(
                        "inline {} fastlang_{}_{}({}) {{",
                        ret_cpp,
                        clean_target,
                        name,
                        param_cpps.join(", ")
                    ));
                    self.indent_level += 1;
                    let prev_prim = self.in_primitive_impl;
                    let prev_target = self.current_primitive_target.clone();
                    self.in_primitive_impl = true;
                    self.current_primitive_target = Some(clean_target.clone());
                    for s in body {
                        self.visit_statement(s);
                    }
                    self.in_primitive_impl = prev_prim;
                    self.current_primitive_target = prev_target;
                    self.indent_level -= 1;
                    self.emit("}");
                }
            }
        }

        self.emit("");

        if wrap_in_main {
            // Find a ScopeDecl named 'main' or fall back to wrapping everything in main
            let has_main_scope = ast.iter().any(|s| {
                if let Stmt::Declaration(Decl::FnDecl { name, .. }) = s {
                    name == "main"
                } else {
                    false
                }
            });

            if !has_main_scope {
                // Wrap all non-class/struct/var/scope statements in int main()
                self.emit("int main() {");
                self.indent_level += 1;
                self.emit("try {");
                self.indent_level += 1;
                for stmt in ast {
                    match stmt {
                        Stmt::Declaration(Decl::ClassDecl { .. })
                        | Stmt::Declaration(Decl::StructDecl { .. })
                        | Stmt::Declaration(Decl::ArrayDecl { .. })
                        | Stmt::Declaration(Decl::VarDecl { .. })
                        | Stmt::Declaration(Decl::DestructureDecl { .. })
                        | Stmt::Declaration(Decl::BlockDecl { .. })
                        | Stmt::Declaration(Decl::MachineDecl { .. })
                        | Stmt::Declaration(Decl::EnumDecl { .. })
                        | Stmt::Declaration(Decl::BlueprintDecl { .. })
                        | Stmt::Declaration(Decl::ImplDecl { .. })
                        | Stmt::Declaration(Decl::FnDecl { .. })
                        | Stmt::Declaration(Decl::NamespaceDecl { .. })
                        | Stmt::Declaration(Decl::MicroDecl { .. })
                        | Stmt::Declaration(Decl::MacroDecl { .. })
                        | Stmt::Declaration(Decl::Import { .. }) => {}
                        _ => {
                            self.visit_statement(stmt);
                        }
                    }
                }
                self.emit("return 0;");
                self.indent_level -= 1;
                self.emit("} catch (const fast_std::Error& __e) {");
                self.indent_level += 1;
                self.emit("std::fprintf(stderr, \"\\n[FastLang Runtime Panic] Uncaught Error: %s\\n\", fastlang_as_str(__e));");
                self.emit("return 1;");
                self.indent_level -= 1;
                self.emit("} catch (const std::exception& __e) {");
                self.indent_level += 1;
                self.emit("std::fprintf(stderr, \"\\n[FastLang Runtime Panic] Fatal Exception: %s\\n\", __e.what());");
                self.emit("return 1;");
                self.indent_level -= 1;
                self.emit("}");
                self.indent_level -= 1;
                self.emit("}");
            }
        }

        self.output.clone()
    }
}

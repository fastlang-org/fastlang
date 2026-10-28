use crate::backend::cpp::generator::CodeGenerator;
use crate::backend::cpp::stmt::{cpp_safe_name, type_to_cpp};
use crate::frontend::parser::ast::*;
use std::mem::take;
impl CodeGenerator {
    pub(crate) fn visit_expression(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::LiteralInt(val) => val.to_string(),
            Expr::LiteralUInt(val) => {
                if *val > i64::MAX as u128 {
                    format!("{}ULL", val)
                } else {
                    val.to_string()
                }
            }
            Expr::LiteralFloat(f) => format!("{:?}", f),
            Expr::LiteralString(s) => {
                let escaped = s
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                format!("fastlang_string_create_from_ptr(\"{}\")", escaped)
            }
            Expr::LiteralUString(s) => {
                let escaped = s
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                format!("fastlang_ustring_create_from_ptr(U\"{}\")", escaped)
            }
            Expr::LiteralChar(c) => match c {
                '\n' => "'\\n'".to_string(),
                '\t' => "'\\t'".to_string(),
                '\r' => "'\\r'".to_string(),
                '\\' => "'\\\\'".to_string(),
                '\'' => "'\\''".to_string(),
                '\0' => "'\\0'".to_string(),
                _ => format!("'{}'", c),
            },
            Expr::LiteralUChar(c) => format!("static_cast<char32_t>({}U)", c),
            Expr::LiteralBool(val) => {
                if *val {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            Expr::LiteralVoid => "fastlang_unit_t{}".to_string(),
            Expr::LiteralUndefined => "fastlang_undefined_t{}".to_string(),
            Expr::ArrayLiteral(elements) => {
                let has_spread = elements.iter().any(|e| matches!(e, Expr::Spread(_)));
                if has_spread {
                    let first = elements
                        .iter()
                        .find(|e| !matches!(e, Expr::Spread(_)))
                        .or_else(|| elements.first());
                    let mut stmts = Vec::new();
                    let first_type_decl = if let Some(f) = first {
                        match f {
                            Expr::Spread(inner) => {
                                let c = self.visit_expression(inner);
                                format!("using __elem_t = std::decay_t<decltype(*({}))>;", c)
                            }
                            _ => {
                                let c = self.visit_expression(f);
                                format!("using __elem_t = std::decay_t<decltype({})>;", c)
                            }
                        }
                    } else {
                        "using __elem_t = int32_t;".to_string()
                    };
                    stmts.push(first_type_decl);
                    stmts.push("fastlang_spread_acc<__elem_t> __acc;".to_string());
                    for el in elements {
                        match el {
                            Expr::Spread(inner) => {
                                let inner_code = self.visit_expression(inner);
                                stmts.push(format!("__acc.push_spread({});", inner_code));
                            }
                            _ => {
                                let el_code = self.visit_expression(el);
                                stmts.push(format!("__acc.push_one({});", el_code));
                            }
                        }
                    }
                    stmts.push("return __acc.to_array();".to_string());
                    format!("([&]() {{\n    {}\n}}())", stmts.join("\n    "))
                } else if elements.is_empty() {
                    "fastlang_empty_array_t{}".to_string()
                } else {
                    let mut elems_code = Vec::new();
                    for el in elements {
                        elems_code.push(self.visit_expression(el));
                    }
                    let first_elem = &elems_code[0];
                    format!(
                        "fastlang_array_create<std::decay_t<decltype({})>>({{{}}})",
                        first_elem,
                        elems_code.join(", ")
                    )
                }
            }
            Expr::Spread(inner) => {
                let inner_code = self.visit_expression(inner);
                if self.variadic_packs.contains(&inner_code) {
                    format!("{}...", inner_code)
                } else {
                    format!("fastlang_spread({})", inner_code)
                }
            }
            Expr::ObjectLiteral(stmts) => {
                let mut struct_code = "([]() { struct __Anon {\n".to_string();
                let old_indent = self.indent_level;
                self.indent_level += 1;
                let captured = self.capture_output(|gen| {
                    for s in stmts {
                        gen.visit_statement(s);
                    }
                });
                self.indent_level = old_indent;
                struct_code.push_str(&captured);
                struct_code.push_str(&format!(
                    "{}}}; return __Anon(); }}())",
                    "    ".repeat(self.indent_level)
                ));
                struct_code
            }
            Expr::Default(type_arg) => {
                if let Some(t) = type_arg {
                    format!("{}()", type_to_cpp(t))
                } else {
                    "{}".to_string()
                }
            }
            Expr::Identifier(name) => {
                if name.starts_with('@') {
                    format!("fastlang_string_create_from_ptr(\"{}\")", name)
                } else {
                    cpp_safe_name(name)
                }
            }
            Expr::This => {
                if self.in_primitive_impl || self.in_machine {
                    "__this".to_string()
                } else {
                    "this".to_string()
                }
            }
            Expr::Super => "super".to_string(), // will be handled in PropertyAccess
            Expr::Global => "::".to_string(),
            Expr::BinaryOp {
                left,
                operator,
                right,
            } => {
                let l = self.visit_expression(left);
                let r = self.visit_expression(right);
                if operator == "->" {
                    let l_type = if let Expr::Identifier(var_name) = &**left {
                        self.vars.get(var_name).cloned()
                    } else if let Expr::PropertyAccess { object, property } = &**left {
                        if matches!(&**object, Expr::This) {
                            self.current_class_name.as_ref().and_then(|cls| {
                                self.struct_field_types
                                    .get(cls)
                                    .and_then(|fields| fields.get(property).cloned())
                            })
                        } else if let Expr::Identifier(obj_name) = &**object {
                            self.vars.get(obj_name.as_str()).and_then(|obj_ty| {
                                let base = Self::clean_type_name(obj_ty);
                                self.struct_field_types
                                    .get(&base)
                                    .and_then(|fields| fields.get(property).cloned())
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let handle = l_type
                        .as_deref()
                        .and_then(|t| self.find_assign_handle(t, &["arrow", "arrow_assign"]));
                    if let Some(h) = handle {
                        format!("{}.{}({})", l, h, r)
                    } else {
                        format!("{}.arrow({})", l, r)
                    }
                } else if operator == "=>" {
                    let l_type = if let Expr::Identifier(var_name) = &**left {
                        self.vars.get(var_name).cloned()
                    } else if let Expr::PropertyAccess { object, property } = &**left {
                        if matches!(&**object, Expr::This) {
                            self.current_class_name.as_ref().and_then(|cls| {
                                self.struct_field_types
                                    .get(cls)
                                    .and_then(|fields| fields.get(property).cloned())
                            })
                        } else if let Expr::Identifier(obj_name) = &**object {
                            self.vars.get(obj_name.as_str()).and_then(|obj_ty| {
                                let base = Self::clean_type_name(obj_ty);
                                self.struct_field_types
                                    .get(&base)
                                    .and_then(|fields| fields.get(property).cloned())
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let handle = l_type
                        .as_deref()
                        .and_then(|t| self.find_assign_handle(t, &["fat_arrow"]));
                    if let Some(h) = handle {
                        format!("{}.{}({})", l, h, r)
                    } else {
                        format!("{}.fat_arrow({})", l, r)
                    }
                } else if operator == "+" {
                    if let Some(target) = self.match_primitive_target(left, "add") {
                        if target == "char" {
                            let rt = self.extract_cpp_type_from_expr(right);
                            let is_array = rt.as_deref().is_some_and(|s| {
                                let resolved = self.resolve_alias(s);
                                resolved == "array"
                                    || resolved.starts_with("array<")
                                    || resolved.starts_with("fast_std::array")
                                    || resolved.ends_with("[]")
                            });
                            if is_array {
                                format!("fastlang_{}_add({}, {})", target, l, r)
                            } else {
                                format!("({} + {})", l, r)
                            }
                        } else {
                            format!("fastlang_{}_add({}, {})", target, l, r)
                        }
                    } else {
                        format!("({} + {})", l, r)
                    }
                } else if operator == "==" {
                    format!("fastlang_eq({}, {})", l, r)
                } else if operator == "!=" {
                    format!("fastlang_ne({}, {})", l, r)
                } else {
                    format!("({} {} {})", l, operator, r)
                }
            }
            Expr::PostfixUpdate { left, operator } => {
                let l = self.visit_expression(left);
                format!("{}{}", l, operator)
            }
            Expr::PrefixUpdate { right, operator } => {
                let r = self.visit_expression(right);
                format!("{}{}", operator, r)
            }
            Expr::IndexAccess { object, indices } => {
                let obj_code = self.visit_expression(object);
                let indices_code: Vec<String> =
                    indices.iter().map(|i| self.visit_expression(i)).collect();
                if self.enum_types.contains(&obj_code) && indices_code.len() == 1 {
                    let variants = self
                        .enum_variant_defs
                        .get(&obj_code)
                        .cloned()
                        .unwrap_or_default();
                    let has_str_disc = variants
                        .iter()
                        .any(|v| matches!(&v.discriminant, Some(Expr::LiteralString(_))));
                    if has_str_disc {
                        let default_var = variants
                            .iter()
                            .find(|v| v.is_default)
                            .or_else(|| variants.last());
                        let mut branches = Vec::new();
                        for v in &variants {
                            if let Some(Expr::LiteralString(s)) = &v.discriminant {
                                if self.simple_enum_types.contains(&obj_code) {
                                    branches.push(format!(
                                        "if (std::strcmp(__fl_disc, \"{}\") == 0) return {}::{};",
                                        s, obj_code, v.name
                                    ));
                                } else {
                                    branches.push(format!(
                                        "if (std::strcmp(__fl_disc, \"{}\") == 0) return {}::{}();",
                                        s, obj_code, v.name
                                    ));
                                }
                            }
                        }
                        let default_ret = if let Some(def) = default_var {
                            if self.simple_enum_types.contains(&obj_code) {
                                format!("return {}::{};", obj_code, def.name)
                            } else {
                                format!("return {}::{}();", obj_code, def.name)
                            }
                        } else {
                            format!("return {}();", obj_code)
                        };
                        return format!(
                            "([&]() {{\n    const char* __fl_disc = fastlang_as_str({});\n    {}\n    {}\n}}())",
                            indices_code[0],
                            branches.join("\n    "),
                            default_ret
                        );
                    } else if self.simple_enum_types.contains(&obj_code) {
                        return format!("static_cast<{}>({})", obj_code, indices_code[0]);
                    } else if self.payload_enum_types.contains(&obj_code) {
                        return format!(
                            "{}(static_cast<{}::Tag>({}))",
                            obj_code, obj_code, indices_code[0]
                        );
                    }
                }
                if indices_code.len() > 1 {
                    format!("{}.index_access({})", obj_code, indices_code.join(", "))
                } else if indices_code.len() == 1 {
                    format!("{}[{}]", obj_code, indices_code[0])
                } else {
                    let chained = indices_code
                        .iter()
                        .map(|i| format!("[{}]", i))
                        .collect::<Vec<_>>()
                        .join("");
                    format!("{}{}", obj_code, chained)
                }
            }
            Expr::UnaryOp { operator, operand } => {
                let op_code = self.visit_expression(operand);
                if operator == "copy" {
                    format!("fastlang_copy(&{})", op_code)
                } else if operator == "*" && self.address_vars.contains(&op_code) {
                    op_code
                } else {
                    format!("{}{}", operator, op_code)
                }
            }
            Expr::MacroCall { callee, args } => {
                let callee_code = self.visit_expression(callee);
                let mut args_code = Vec::new();
                for arg in args {
                    args_code.push(self.visit_expression(arg));
                }
                format!("{}({})", callee_code, args_code.join(", "))
            }

            Expr::Call {
                callee,
                generics,
                args,
            } => {
                if let Expr::IndexAccess { object, indices } = &**callee {
                    let obj_code = self.visit_expression(object);
                    if (self.enum_types.contains(&obj_code)
                        || self.payload_enum_types.contains(&obj_code))
                        && indices.len() == 1
                    {
                        let idx_code = self.visit_expression(&indices[0]);
                        let variants = self
                            .enum_variant_defs
                            .get(&obj_code)
                            .cloned()
                            .unwrap_or_default();
                        let default_var = variants
                            .iter()
                            .find(|v| v.is_default)
                            .or_else(|| variants.last());

                        let mut arg_evals = Vec::new();
                        let mut arg_names = Vec::new();
                        for (i, arg) in args.iter().enumerate() {
                            let arg_val = self.visit_expression(arg);
                            let arg_name = format!("__fl_arg_{}", i);
                            arg_evals.push(format!("auto {} = {};", arg_name, arg_val));
                            arg_names.push(arg_name);
                        }
                        let args_forward = arg_names.join(", ");

                        let has_str_disc = variants
                            .iter()
                            .any(|v| matches!(&v.discriminant, Some(Expr::LiteralString(_))));

                        let mut branches = Vec::new();
                        for v in &variants {
                            if let Some(ref disc) = v.discriminant {
                                if let Expr::LiteralString(s) = disc {
                                    branches.push(format!(
                                        "if (std::strcmp(__fl_disc, \"{}\") == 0) return {}::{}({});",
                                        s, obj_code, v.name, args_forward
                                    ));
                                } else if let Expr::LiteralInt(i) = disc {
                                    branches.push(format!(
                                        "if (__fl_disc == {}) return {}::{}({});",
                                        i, obj_code, v.name, args_forward
                                    ));
                                }
                            }
                        }

                        let default_ret = if let Some(def) = default_var {
                            format!("return {}::{}({});", obj_code, def.name, args_forward)
                        } else {
                            format!("return {}();", obj_code)
                        };

                        let disc_init = if has_str_disc {
                            format!("const char* __fl_disc = fastlang_as_str({});", idx_code)
                        } else {
                            format!("auto __fl_disc = {};", idx_code)
                        };

                        return format!(
                            "([&]() {{\n    {}\n    {}\n    {}\n    {}\n}}())",
                            disc_init,
                            arg_evals.join("\n    "),
                            branches.join("\n    "),
                            default_ret
                        );
                    }
                }

                if let Expr::PropertyAccess { object, property } = &**callee {
                    if property == "default" {
                        if let Some(cpp_t) = self.extract_cpp_type_from_expr(object) {
                            return format!("fastlang_type_default<{}>()", cpp_t);
                        }
                    }
                    if property == "as_str" && args.is_empty() {
                        if matches!(**object, Expr::This) {
                            if self.in_primitive_impl {
                                let target =
                                    self.current_primitive_target.as_deref().unwrap_or("array");
                                return format!("fastlang_{}_as_str(__this)", target);
                            }
                            return "this->as_str()".to_string();
                        }
                        let obj_code = self.visit_expression(object);
                        return format!("fastlang_as_str({})", obj_code);
                    }
                    if property == "copy" && args.is_empty() {
                        let obj_code = self.visit_expression(object);
                        return format!("fastlang_copy({})", obj_code);
                    }

                    if let Some(targets) = self.primitive_impl_methods.get(property).cloned() {
                        let obj_code = self.visit_expression(object);
                        let cpp_type = self.extract_cpp_type_from_expr(object).unwrap_or_default();
                        let resolved_type = self.resolve_alias(&cpp_type);
                        let is_this = matches!(**object, Expr::This);

                        let matched_target = if is_this && self.in_primitive_impl {
                            self.current_primitive_target.clone()
                        } else if targets
                            .iter()
                            .any(|t| t == &cpp_type || t == &resolved_type)
                        {
                            Some(if targets.contains(&cpp_type) {
                                cpp_type.clone()
                            } else {
                                resolved_type.clone()
                            })
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
                        } else if targets.contains(&"char".to_string())
                            && (cpp_type == "char" || resolved_type == "char")
                        {
                            Some("char".to_string())
                        } else if targets.contains(&"bool".to_string())
                            && (cpp_type == "bool"
                                || cpp_type == "flag"
                                || resolved_type == "bool"
                                || resolved_type == "flag")
                        {
                            Some("bool".to_string())
                        } else {
                            None
                        };

                        if let Some(target) = matched_target {
                            let mut args_code = Vec::new();
                            for arg in args {
                                args_code.push(self.visit_expression(arg));
                            }
                            let mut all_args = vec![obj_code];
                            all_args.extend(args_code);
                            return format!(
                                "fastlang_{}_{}({})",
                                target,
                                property,
                                all_args.join(", ")
                            );
                        }
                    }

                    // UFCS dispatch: if `property` is not a member of `object`, but is a free function
                    let clean_obj_ty = self
                        .extract_cpp_type_from_expr(object)
                        .map(|t| Self::clean_type_name(&t))
                        .unwrap_or_default();
                    let is_member = if clean_obj_ty.is_empty() {
                        false
                    } else {
                        self.type_own_members
                            .get(&clean_obj_ty)
                            .is_some_and(|m| m.contains(property))
                            || self
                                .struct_field_types
                                .get(&clean_obj_ty)
                                .is_some_and(|f| f.contains_key(property))
                    };
                    let obj_code_preview = self.visit_expression(object);
                    let is_this_or_super = obj_code_preview == "this"
                        || obj_code_preview == "super"
                        || obj_code_preview == "::"
                        || self.enum_types.contains(&obj_code_preview);
                    let is_free_fn = self.forward_declared_fns.contains(property)
                        || self.fn_return_types.contains_key(property);

                    let mut is_compatible_first_arg = false;
                    let mut is_first_arg_pointer = false;
                    if let Some(overloads) = self.fn_param_types.get(property) {
                        for pts in overloads {
                            if let Some(first_pt) = pts.first() {
                                let clean_pt = Self::clean_type_name(first_pt);
                                if clean_obj_ty.is_empty()
                                    || clean_pt == clean_obj_ty
                                    || (clean_pt.contains("int") && clean_obj_ty.contains("int"))
                                    || (clean_pt.contains("float") && clean_obj_ty.contains("float"))
                                    || (clean_pt == "double" && (clean_obj_ty == "double" || clean_obj_ty.contains("float")))
                                    || clean_pt == "auto"
                                    || clean_pt == "any"
                                {
                                    is_compatible_first_arg = true;
                                    if first_pt.ends_with('*') {
                                        is_first_arg_pointer = true;
                                    }
                                    break;
                                }
                            }
                        }
                    } else if is_free_fn {
                        is_compatible_first_arg = true;
                    }

                    if !is_member && !is_this_or_super && is_free_fn && is_compatible_first_arg {
                        let mut first_arg = obj_code_preview;
                        if is_first_arg_pointer
                            && !first_arg.starts_with('&')
                            && !self.raw_pointer_vars.contains(&first_arg)
                            && !self.pointer_vars.contains(&first_arg)
                        {
                            first_arg = format!("&{}", first_arg);
                        }
                        let mut all_args = vec![first_arg];
                        for arg in args {
                            all_args.push(self.visit_expression(arg));
                        }
                        let safe_prop = cpp_safe_name(property);
                        if !generics.is_empty() {
                            let gen_strs: Vec<String> = generics.iter().map(type_to_cpp).collect();
                            return format!(
                                "{}<{}>({})",
                                safe_prop,
                                gen_strs.join(", "),
                                all_args.join(", ")
                            );
                        } else {
                            return format!("{}({})", safe_prop, all_args.join(", "));
                        }
                    }
                }

                let callee_code = match &**callee {
                    Expr::PropertyAccess { object, property } => {
                        let obj_code = self.visit_expression(object);
                        let safe_prop = cpp_safe_name(property);
                        if obj_code == "::" {
                            format!("::{}", safe_prop)
                        } else if self.enum_types.contains(&obj_code) {
                            format!("{}::{}", obj_code, safe_prop)
                        } else if obj_code == "super" || obj_code == "this" {
                            format!("this->{}", safe_prop)
                        } else if self.raw_pointer_vars.contains(&obj_code) {
                            format!("{}->{}", obj_code, safe_prop)
                        } else if let Some(t) = self.extract_cpp_type_from_expr(object) {
                            if t.ends_with('*')
                                && t != "char*"
                                && t != "const char*"
                                && t != "char32_t*"
                                && t != "const char32_t*"
                                && t != "str"
                                && t != "ustr"
                            {
                                format!("{}->{}", obj_code, safe_prop)
                            } else {
                                format!("{}.{}", obj_code, safe_prop)
                            }
                        } else {
                            format!("{}.{}", obj_code, safe_prop)
                        }
                    }
                    Expr::NamespaceAccess {
                        namespace,
                        property,
                    } => {
                        let prop_code = self.visit_expression(property);
                        let clean_prop = prop_code.trim_end_matches("()");
                        if namespace == "@compile" {
                            if clean_prop == "time" {
                                return "Time::now()".to_string();
                            }
                            if clean_prop == "backend" {
                                return "Backend::Cpp".to_string();
                            }
                            if clean_prop == "system" {
                                let sys = if cfg!(target_os = "windows") {
                                    "System::Windows"
                                } else if cfg!(target_os = "macos") {
                                    "System::MacOS"
                                } else {
                                    "System::Linux"
                                };
                                return sys.to_string();
                            }
                            clean_prop.to_string()
                        } else if namespace == "@c" || namespace == "@cpp" {
                            clean_prop.to_string()
                        } else if namespace == "@sys" {
                            format!("fast_{}", clean_prop)
                        } else {
                            let effective_ns = if namespace.starts_with("std::") {
                                namespace.replacen("std::", "fast_std::", 1)
                            } else if namespace == "std" {
                                "fast_std".to_string()
                            } else {
                                namespace.clone()
                            };
                            if self.enum_types.contains(namespace)
                                || self.payload_enum_types.contains(namespace)
                                || self.struct_field_types.contains_key(namespace)
                                || self.is_known_namespace(namespace)
                                || self.is_known_namespace(&effective_ns)
                                || namespace.chars().next().is_some_and(|c| c.is_uppercase())
                            {
                                format!("{}::{}", effective_ns, clean_prop)
                            } else {
                                clean_prop.to_string()
                            }
                        }
                    }
                    Expr::Identifier(name) if name.starts_with('@') => cpp_safe_name(name),
                    _ => self.visit_expression(callee),
                };

                let mut args_code = Vec::new();
                for arg in args {
                    args_code.push(self.visit_expression(arg));
                }

                let gen_code = if !generics.is_empty() {
                    let gen_strs: Vec<String> = generics.iter().map(type_to_cpp).collect();
                    format!("<{}>", gen_strs.join(", "))
                } else {
                    "".to_string()
                };

                let safe_callee = match &**callee {
                    Expr::UnaryOp { .. } => format!("({})", callee_code),
                    _ => callee_code,
                };

                if self.machine_types.contains(&safe_callee) {
                    format!("{}{}()({})", safe_callee, gen_code, args_code.join(", "))
                } else if self.pointer_vars.contains(&safe_callee) {
                    format!("(*{}){}({})", safe_callee, gen_code, args_code.join(", "))
                } else {
                    format!("{}{}({})", safe_callee, gen_code, args_code.join(", "))
                }
            }
            Expr::Instantiate { target, args } => {
                let target_code = self.visit_expression(target);
                if args.len() == 1 && matches!(args[0], Expr::ObjectLiteral(_)) {
                    if let Expr::ObjectLiteral(ref stmts) = args[0] {
                        if target_code.contains("::") {
                            let parts: Vec<&str> = target_code.split("::").collect();
                            if parts.len() == 2 && self.enum_types.contains(parts[0]) {
                                let enum_name = parts[0];
                                let variant_name = parts[1].trim_end_matches("()");
                                let mut struct_code = format!(
                                    "([&]() {{\n    typename {}::{}_Payload __payload;\n",
                                    enum_name, variant_name
                                );
                                let old_indent = self.indent_level;
                                self.indent_level += 1;
                                let captured = self.capture_output(|gen| {
                                    for s in stmts {
                                        match s {
                                            Stmt::Declaration(Decl::VarDecl {
                                                name,
                                                value,
                                                assign_op,
                                                ..
                                            }) => {
                                                let v = gen.visit_expression(value);
                                                if !matches!(value, Expr::Default(None))
                                                    && v != "{}"
                                                {
                                                    let op = if assign_op.is_empty() {
                                                        "="
                                                    } else {
                                                        assign_op.as_str()
                                                    };
                                                    gen.emit(&format!(
                                                        "__payload.{} {} {};",
                                                        name, op, v
                                                    ));
                                                }
                                            }
                                            Stmt::ReassignStmt { target, value, op } => {
                                                let t = gen.visit_expression(target);
                                                let v = gen.visit_expression(value);
                                                gen.emit(&format!("__payload.{} {} {};", t, op, v));
                                            }
                                            _ => {
                                                gen.visit_statement(s);
                                            }
                                        }
                                    }
                                });
                                self.indent_level = old_indent;
                                struct_code.push_str(&captured);
                                struct_code.push_str(&format!(
                                    "{} {} __obj({}::__no_init_t{{}});\n{}__obj.tag = {}::Tag::{};\n{}new (&__obj.storage.v_{}) typename {}::{}_Payload(std::move(__payload));\n{}return __obj;\n{}}}())",
                                    "    ".repeat(self.indent_level + 1),
                                    enum_name,
                                    enum_name,
                                    "    ".repeat(self.indent_level + 1),
                                    enum_name,
                                    variant_name,
                                    "    ".repeat(self.indent_level + 1),
                                    variant_name,
                                    enum_name,
                                    variant_name,
                                    "    ".repeat(self.indent_level + 1),
                                    "    ".repeat(self.indent_level)
                                ));
                                return struct_code;
                            }
                        }
                        let mut struct_code = format!("([&]() {{\n    {} __obj;\n", target_code);
                        let old_indent = self.indent_level;
                        self.indent_level += 1;
                        let captured = self.capture_output(|gen| {
                            for s in stmts {
                                match s {
                                    Stmt::Declaration(Decl::VarDecl {
                                        name,
                                        value,
                                        assign_op,
                                        ..
                                    }) => {
                                        let v = gen.visit_expression(value);
                                        if !matches!(value, Expr::Default(None)) && v != "{}" {
                                            let op = if assign_op.is_empty() {
                                                "="
                                            } else {
                                                assign_op.as_str()
                                            };
                                            gen.emit(&format!("__obj.{} {} {};", name, op, v));
                                        }
                                    }
                                    Stmt::ReassignStmt { target, value, op } => {
                                        let t = gen.visit_expression(target);
                                        let v = gen.visit_expression(value);
                                        gen.emit(&format!("__obj.{} {} {};", t, op, v));
                                    }
                                    _ => {
                                        gen.visit_statement(s);
                                    }
                                }
                            }
                        });
                        self.indent_level = old_indent;
                        struct_code.push_str(&captured);
                        struct_code.push_str(&format!(
                            "{}return __obj;\n{}}}())",
                            "    ".repeat(self.indent_level + 1),
                            "    ".repeat(self.indent_level)
                        ));
                        return struct_code;
                    }
                }
                let mut args_code = Vec::new();
                for arg in args {
                    args_code.push(self.visit_expression(arg));
                }
                // Value instantiation in C++ (stack allocation)
                format!("{}({})", target_code, args_code.join(", "))
            }
            Expr::PropertyAccess { object, property } => {
                if property == "default" {
                    if let Some(cpp_t) = self.extract_cpp_type_from_expr(object) {
                        return format!("fastlang_type_default<{}>()", cpp_t);
                    }
                }
                if property == "len" {
                    let obj_code = self.visit_expression(object);
                    return format!("fastlang_array_len({})", obj_code);
                }
                let obj_code = self.visit_expression(object);
                let safe_prop = cpp_safe_name(property);
                if obj_code == "::" {
                    format!("::{}", safe_prop)
                } else if self.enum_types.contains(&obj_code) {
                    format!("{}::{}", obj_code, safe_prop)
                } else if obj_code == "super" || obj_code == "this" {
                    format!("this->{}", safe_prop)
                } else if self.raw_pointer_vars.contains(&obj_code) {
                    format!("{}->{}", obj_code, safe_prop)
                } else if let Some(t) = self.extract_cpp_type_from_expr(object) {
                    if t.ends_with('*')
                        && t != "char*"
                        && t != "const char*"
                        && t != "char32_t*"
                        && t != "const char32_t*"
                        && t != "str"
                        && t != "ustr"
                    {
                        format!("{}->{}", obj_code, safe_prop)
                    } else {
                        format!("{}.{}", obj_code, safe_prop)
                    }
                } else {
                    format!("{}.{}", obj_code, safe_prop)
                }
            }
            Expr::NamespaceAccess {
                namespace,
                property,
            } => {
                let prop_code = self.visit_expression(property);
                if namespace == "@compile" {
                    if prop_code == "backend" {
                        "Backend::Cpp".to_string()
                    } else if prop_code == "system" {
                        let sys = if cfg!(target_os = "windows") {
                            "System::Windows"
                        } else if cfg!(target_os = "macos") {
                            "System::MacOS"
                        } else {
                            "System::Linux"
                        };
                        sys.to_string()
                    } else {
                        prop_code
                    }
                } else if namespace == "@c" || namespace == "@cpp" {
                    prop_code
                } else if namespace == "@sys" {
                    format!("fast_{}", prop_code)
                } else if namespace.starts_with('@') {
                    format!("this->{}", prop_code)
                } else if self.payload_enum_types.contains(namespace) {
                    format!("{}::{}()", namespace, prop_code)
                } else {
                    let effective_ns = if namespace.starts_with("std::") {
                        namespace.replacen("std::", "fast_std::", 1)
                    } else if namespace == "std" {
                        "fast_std".to_string()
                    } else {
                        namespace.clone()
                    };
                    let clean_prop = prop_code.trim_end_matches("()");
                    if self.enum_types.contains(namespace)
                        || self.struct_field_types.contains_key(namespace)
                        || self.is_known_namespace(namespace)
                        || self.is_known_namespace(&effective_ns)
                        || namespace.chars().next().is_some_and(|c| c.is_uppercase())
                    {
                        format!("{}::{}", effective_ns, clean_prop)
                    } else {
                        clean_prop.to_string()
                    }
                }
            }
            Expr::ArrayAllocate {
                type_node,
                size,
                length,
            } => {
                let cpp_type: String = type_to_cpp(type_node);
                if let Some(init) = length {
                    let init_code = self.visit_expression(init);
                    format!("fastlang_array_create<{}>({})", cpp_type, init_code)
                } else {
                    let size_code = self.visit_expression(size);
                    format!(
                        "fastlang_array_alloc<{}>((size_t)({}))",
                        cpp_type, size_code
                    )
                }
            }
            Expr::New { type_node, target } => {
                if let BaseType::Array { base_type, size } = type_node {
                    let cpp_elem = type_to_cpp(base_type);
                    if let Some(s) = size.as_ref() {
                        let size_code = self.visit_expression(s);
                        return format!(
                            "fastlang_array_alloc<{}>((size_t)({}))",
                            cpp_elem, size_code
                        );
                    } else {
                        return format!("fastlang_array_alloc<{}>(0)", cpp_elem);
                    }
                }
                let cpp_type = type_to_cpp(type_node);
                match &**target {
                    Expr::Instantiate { args, .. } => {
                        let arg_strs: Vec<String> =
                            args.iter().map(|a| self.visit_expression(a)).collect();
                        format!("new {}({})", cpp_type, arg_strs.join(", "))
                    }
                    Expr::ArrayLiteral(elems) => {
                        let elem_strs: Vec<String> =
                            elems.iter().map(|e| self.visit_expression(e)).collect();
                        format!("new {}[]{{{}}}", cpp_type, elem_strs.join(", "))
                    }
                    _ => {
                        let target_code = self.visit_expression(target);
                        if target_code == "__default__"
                            || target_code == "{}"
                            || target_code.is_empty()
                        {
                            format!("new {}()", cpp_type)
                        } else {
                            format!("new {}({})", cpp_type, target_code)
                        }
                    }
                }
            }
            Expr::Lambda {
                params,
                return_type,
                body,
                ..
            } => {
                let p_list: Vec<String> = params
                    .iter()
                    .map(|p| {
                        if p.type_node == BaseType::Unknown
                            || matches!(p.type_node, BaseType::GenericParam(_))
                        {
                            format!("auto {}", p.name)
                        } else {
                            format!("{} {}", type_to_cpp(&p.type_node), p.name)
                        }
                    })
                    .collect();
                let ret_str = if let Some(rt) = return_type {
                    format!(" -> {}", type_to_cpp(rt))
                } else {
                    "".to_string()
                };

                let old_indent = self.indent_level;
                self.indent_level += 1;
                let captured = self.capture_output(|gen| {
                    for s in body {
                        gen.visit_statement(s);
                    }
                });
                self.indent_level = old_indent;

                format!(
                    "([&]({}){} {{\n{}{}}})",
                    p_list.join(", "),
                    ret_str,
                    captured,
                    "    ".repeat(self.indent_level)
                )
            }
            Expr::Cast { expr, target_type } => {
                let expr_code = self.visit_expression(expr);
                let cpp_type = type_to_cpp(target_type);
                if matches!(target_type, BaseType::Str) {
                    format!("fastlang_as_str({})", expr_code)
                } else if matches!(target_type, BaseType::UStr) {
                    format!("fastlang_as_ustr({})", expr_code)
                } else if let Some(target) = self.match_primitive_target(expr, "cast") {
                    let template_arg = cpp_type;
                    format!("fastlang_{}_cast<{}>({})", target, template_arg, expr_code)
                } else {
                    format!("static_cast<{}>({})", cpp_type, expr_code)
                }
            }
            Expr::HandleCall {
                object,
                handle_name,
                args,
            } => {
                let obj_code = self.visit_expression(object);
                let args_code: Vec<String> =
                    args.iter().map(|a| self.visit_expression(a)).collect();
                let safe_handle = cpp_safe_name(handle_name);

                if (handle_name == "display" || handle_name == "as_str") && args.is_empty() {
                    let cpp_t = self.extract_cpp_type_from_expr(object).unwrap_or_default();
                    if cpp_t == "char*" || cpp_t == "const char*" || cpp_t == "str" {
                        return obj_code;
                    }
                    if self.raw_pointer_vars.contains(&obj_code) {
                        return format!("{}->display()", obj_code);
                    }
                    if !cpp_t.is_empty()
                        && !matches!(
                            cpp_t.as_str(),
                            "int"
                                | "int32_t"
                                | "int64_t"
                                | "int16_t"
                                | "int8_t"
                                | "uint32_t"
                                | "uint64_t"
                                | "size_t"
                                | "uint16_t"
                                | "uint8_t"
                                | "float"
                                | "double"
                                | "bool"
                                | "char"
                        )
                    {
                        return format!("{}.display()", obj_code);
                    }
                    return format!("fastlang_as_str({})", obj_code);
                }
                if handle_name == "copy" && args.is_empty() {
                    return format!("fastlang_copy({})", obj_code);
                }

                if let Some(target) = self.match_primitive_target(object, handle_name) {
                    let mut all_args = vec![obj_code];
                    all_args.extend(args_code);
                    return format!(
                        "fastlang_{}_{}({})",
                        target,
                        handle_name,
                        all_args.join(", ")
                    );
                }

                if obj_code == "super" || obj_code == "this" {
                    format!("this->{}({})", safe_handle, args_code.join(", "))
                } else if self.raw_pointer_vars.contains(&obj_code) {
                    format!("{}->{}({})", obj_code, safe_handle, args_code.join(", "))
                } else {
                    format!("{}.{}({})", obj_code, safe_handle, args_code.join(", "))
                }
            }
            Expr::IfExpr {
                condition,
                then_branch,
                else_branch,
            } => {
                let has_block = matches!(**then_branch, Expr::BlockExpr { .. })
                    || matches!(**else_branch, Expr::BlockExpr { .. });
                if !has_block {
                    let cond_code = self.visit_expression(condition);
                    let then_code = self.visit_expression(then_branch);
                    let else_code = self.visit_expression(else_branch);
                    format!("(({}) ? ({}) : ({}))", cond_code, then_code, else_code)
                } else {
                    let cond_code = self.visit_expression(condition);
                    let old_out = take(&mut self.output);
                    let old_indent = self.indent_level;
                    self.indent_level += 2;

                    let then_code = match &**then_branch {
                        Expr::BlockExpr {
                            statements,
                            final_expr,
                        } => {
                            for s in statements {
                                self.visit_statement(s);
                            }
                            if let Some(f) = final_expr {
                                let fc = self.visit_expression(f);
                                self.emit(&format!("return {};", fc));
                            }
                            take(&mut self.output)
                        }
                        _ => {
                            let tc = self.visit_expression(then_branch);
                            self.emit(&format!("return {};", tc));
                            take(&mut self.output)
                        }
                    };

                    let else_code = match &**else_branch {
                        Expr::BlockExpr {
                            statements,
                            final_expr,
                        } => {
                            for s in statements {
                                self.visit_statement(s);
                            }
                            if let Some(f) = final_expr {
                                let fc = self.visit_expression(f);
                                self.emit(&format!("return {};", fc));
                            }
                            take(&mut self.output)
                        }
                        _ => {
                            let ec = self.visit_expression(else_branch);
                            self.emit(&format!("return {};", ec));
                            take(&mut self.output)
                        }
                    };

                    self.indent_level = old_indent;
                    self.output = old_out;

                    let base_indent = "    ".repeat(self.indent_level);
                    let branch_indent = "    ".repeat(self.indent_level + 1);

                    format!(
                        "([&]() {{\n{}if ({}) {{\n{}{}}} else {{\n{}{}}}\n{}}}())",
                        branch_indent,
                        cond_code,
                        then_code,
                        branch_indent,
                        else_code,
                        branch_indent,
                        base_indent
                    )
                }
            }
            Expr::BlockExpr {
                statements,
                final_expr,
            } => {
                let old_out = take(&mut self.output);
                let old_indent = self.indent_level;
                self.indent_level += 1;

                for s in statements {
                    self.visit_statement(s);
                }
                if let Some(f) = final_expr {
                    let fc = self.visit_expression(f);
                    self.emit(&format!("return {};", fc));
                }

                let body_code = take(&mut self.output);
                self.indent_level = old_indent;
                self.output = old_out;

                let base_indent = "    ".repeat(self.indent_level);
                format!("([&]() {{\n{}{}}}())", body_code, base_indent)
            }
            Expr::InPlaceBlock { body } => {
                let old_out = take(&mut self.output);
                let old_indent = self.indent_level;
                self.indent_level += 1;

                for s in body {
                    self.visit_statement(s);
                }

                let body_code = take(&mut self.output);
                self.indent_level = old_indent;
                self.output = old_out;

                let base_indent = "    ".repeat(self.indent_level);
                format!("([&]() {{\n{}{}}}())", body_code, base_indent)
            }
            Expr::QuestionMark(inner) => {
                let inner_code = self.visit_expression(inner);
                let base_indent = "    ".repeat(self.indent_level);
                format!(
                    "([&]() {{\n{}    try {{\n{}        return ({});\n{}    }} catch (const fast_std::Error& __e) {{\n{}        throw __e;\n{}    }}\n{}}}())",
                    base_indent, base_indent, inner_code, base_indent, base_indent, base_indent, base_indent
                )
            }
            Expr::ForeignBlock { kind, chunks } => {
                let old_out = take(&mut self.output);
                let old_indent = self.indent_level;
                self.indent_level += 1;

                self.emit("fastlang_strbuf __buf;");
                self.emit("auto __append = [&](const char* s) { __buf.append(s); };");

                let mut prev_ended_with_equal = false;

                for chunk in chunks {
                    match chunk {
                        ForeignChunk::Static(s) => {
                            let processed_s = if matches!(kind, ForeignKind::Html) {
                                transform_fastreact_html(s)
                            } else {
                                s.clone()
                            };
                            let escaped = processed_s
                                .replace('\\', "\\\\")
                                .replace('"', "\\\"")
                                .replace('\n', "\\n")
                                .replace('\r', "\\r")
                                .replace('\t', "\\t");
                            self.emit(&format!("__append(\"{}\");", escaped));
                            let trimmed = processed_s.trim_end();
                            prev_ended_with_equal =
                                matches!(kind, ForeignKind::Html) && trimmed.ends_with('=');
                        }
                        ForeignChunk::Interpolation(expr) => {
                            let expr_code = self.visit_expression(expr);
                            if prev_ended_with_equal {
                                self.emit("__append(\"\\\"\");");
                                self.emit(&format!("__append({});", expr_code));
                                self.emit("__append(\"\\\"\");");
                                prev_ended_with_equal = false;
                            } else {
                                self.emit(&format!("__append({});", expr_code));
                            }
                        }
                        ForeignChunk::Statements(stmts) => {
                            prev_ended_with_equal = false;
                            for stmt in stmts {
                                self.visit_statement(stmt);
                            }
                        }
                    }
                }

                match kind {
                    ForeignKind::Html => {
                        self.emit("fast_std::HtmlDocument __doc;");
                        self.emit("__doc.set_content(__buf.finalize());");
                        self.emit("return __doc;");
                    }
                    ForeignKind::Json => {
                        self.emit("fast_std::JsonDocument __doc;");
                        self.emit("__doc.set_content(__buf.finalize());");
                        self.emit("return __doc;");
                    }
                    ForeignKind::Safe | ForeignKind::Raw => {
                        self.emit("return __buf.finalize();");
                    }
                }

                let body_code = take(&mut self.output);
                self.indent_level = old_indent;
                self.output = old_out;

                let base_indent = "    ".repeat(self.indent_level);
                format!("([&]() {{\n{}{}}}())", body_code, base_indent)
            }
        }
    }

    pub(crate) fn extract_cpp_type_from_expr(&mut self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Identifier(name) => {
                if let Some(tracked_type) = self.vars.get(name) {
                    return Some(tracked_type.clone());
                }
                let bt = BaseType::from_str(name);
                if bt != BaseType::Unknown {
                    Some(type_to_cpp(&bt))
                } else {
                    Some(cpp_safe_name(name))
                }
            }
            Expr::IndexAccess { object, .. } => {
                if matches!(**object, Expr::This) {
                    Some("char".to_string())
                } else if let Some(parent_type) = self.extract_cpp_type_from_expr(object) {
                    if parent_type.starts_with("fast_std::array<") && parent_type.ends_with('>') {
                        Some(
                            parent_type["fast_std::array<".len()..parent_type.len() - 1]
                                .to_string(),
                        )
                    } else if parent_type.starts_with("array<") && parent_type.ends_with('>') {
                        Some(parent_type["array<".len()..parent_type.len() - 1].to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Expr::Call { callee, .. } => {
                if let Expr::PropertyAccess { object, property } = &**callee {
                    if property == "chain"
                        || property == "slice"
                        || property == "sort"
                        || property == "reverse"
                    {
                        self.extract_cpp_type_from_expr(object)
                    } else if property == "as_str" || property == "to_str" || property == "display"
                    {
                        Some("fast_std::array<char>".to_string())
                    } else {
                        None
                    }
                } else if let Expr::Identifier(fn_name) = &**callee {
                    self.fn_return_types.get(fn_name).cloned()
                } else {
                    None
                }
            }
            Expr::PropertyAccess { object, property } => {
                if matches!(**object, Expr::This) {
                    if let Some(cls) = &self.current_class_name {
                        if let Some(fields) = self.struct_field_types.get(cls) {
                            if let Some(ft) = fields.get(property) {
                                return Some(ft.clone());
                            }
                        }
                    }
                    if let Some(ft) = self.vars.get(&format!("this->{}", property)) {
                        return Some(ft.clone());
                    }
                    if let Some(ft) = self.vars.get(property) {
                        return Some(ft.clone());
                    }
                } else if let Some(parent_type) = self.extract_cpp_type_from_expr(object) {
                    let clean_target = parent_type
                        .trim_start_matches("fast_std::")
                        .trim_end_matches('*')
                        .split('<')
                        .next()
                        .unwrap_or(&parent_type);
                    if let Some(fields) = self.struct_field_types.get(clean_target) {
                        if let Some(ft) = fields.get(property) {
                            return Some(ft.clone());
                        }
                    }
                }

                None
            }
            Expr::LiteralString(_) => Some("char*".to_string()),
            Expr::LiteralUString(_) => Some("char32_t*".to_string()),
            Expr::ArrayLiteral(_) => Some("array".to_string()),
            Expr::LiteralChar(_) => Some("char".to_string()),
            Expr::LiteralBool(_) => Some("bool".to_string()),
            Expr::LiteralInt(_) => Some("int32_t".to_string()),
            Expr::LiteralFloat(_) => Some("double".to_string()),
            Expr::IfExpr {
                then_branch,
                else_branch,
                ..
            } => self
                .extract_cpp_type_from_expr(then_branch)
                .or_else(|| self.extract_cpp_type_from_expr(else_branch)),
            Expr::BlockExpr { final_expr, .. } => final_expr
                .as_ref()
                .and_then(|f| self.extract_cpp_type_from_expr(f)),
            Expr::Cast { target_type, .. } => Some(type_to_cpp(target_type)),
            Expr::BinaryOp {
                left,
                operator,
                right,
            } => {
                if operator == "+" {
                    let lt = self.extract_cpp_type_from_expr(left);
                    let rt = self.extract_cpp_type_from_expr(right);
                    let is_array = |t: &Option<String>| {
                        if let Some(s) = t {
                            let resolved = self.resolve_alias(s);
                            resolved == "array"
                                || resolved.starts_with("array<")
                                || resolved.starts_with("fast_std::array")
                                || resolved.ends_with("[]")
                        } else {
                            false
                        }
                    };
                    if is_array(&lt) {
                        lt
                    } else if is_array(&rt) {
                        rt
                    } else {
                        lt.or(rt)
                    }
                } else {
                    self.extract_cpp_type_from_expr(left)
                }
            }
            Expr::QuestionMark(inner) => self.extract_cpp_type_from_expr(inner),
            Expr::ForeignBlock { kind, .. } => match kind {
                ForeignKind::Html => Some("fast_std::HtmlDocument".to_string()),
                ForeignKind::Json => Some("fast_std::JsonDocument".to_string()),
                ForeignKind::Safe | ForeignKind::Raw => Some("str".to_string()),
            },
            _ => None,
        }
    }
}

pub(crate) fn transform_fastreact_html(input: &str) -> String {
    let mut out = input.to_string();

    // 1. onclick="/..." or onsubmit="/..."
    for attr in &["onclick", "onsubmit"] {
        for prefix in &[
            format!(" {}=\"/", attr),
            format!("\n{}=\"/", attr),
            format!("\t{}=\"/", attr),
            format!(" {}='/", attr),
            format!("\n{}='/", attr),
            format!("\t{}='/", attr),
        ] {
            let repl = if prefix.starts_with('\n') {
                if prefix.ends_with('\'') {
                    "\nhx-post='/"
                } else {
                    "\nhx-post=\"/"
                }
            } else if prefix.starts_with('\t') {
                if prefix.ends_with('\'') {
                    "\thx-post='/"
                } else {
                    "\thx-post=\"/"
                }
            } else {
                if prefix.ends_with('\'') {
                    " hx-post='/"
                } else {
                    " hx-post=\"/"
                }
            };
            out = out.replace(prefix.as_str(), repl);
        }
        // trailing onclick= or onsubmit= before interpolation
        out = out.replace(&format!(" {}=", attr), " hx-post=");
        out = out.replace(&format!("\n{}=", attr), "\nhx-post=");
        out = out.replace(&format!("\t{}=", attr), "\thx-post=");
    }

    // 2. target="..." (exclude _blank, _self, _top, _parent)
    for prefix in &[
        " target=\"",
        "\ntarget=\"",
        "\ttarget=\"",
        " target='",
        "\ntarget='",
        "\ttarget='",
    ] {
        let mut start_idx = 0;
        while let Some(pos) = out[start_idx..].find(prefix) {
            let actual_pos = start_idx + pos;
            let after_prefix = actual_pos + prefix.len();
            if !out[after_prefix..].starts_with("_blank")
                && !out[after_prefix..].starts_with("_self")
                && !out[after_prefix..].starts_with("_top")
                && !out[after_prefix..].starts_with("_parent")
            {
                let repl = if prefix.starts_with('\n') {
                    if prefix.ends_with('\'') {
                        "\nhx-target='"
                    } else {
                        "\nhx-target=\""
                    }
                } else if prefix.starts_with('\t') {
                    if prefix.ends_with('\'') {
                        "\thx-target='"
                    } else {
                        "\thx-target=\""
                    }
                } else {
                    if prefix.ends_with('\'') {
                        " hx-target='"
                    } else {
                        " hx-target=\""
                    }
                };
                out.replace_range(actual_pos..actual_pos + prefix.len(), repl);
                start_idx = actual_pos + repl.len();
            } else {
                start_idx = after_prefix;
            }
        }
    }
    // trailing target= before interpolation
    out = out.replace(" target=", " hx-target=");
    out = out.replace("\ntarget=", "\nhx-target=");
    out = out.replace("\ttarget=", "\thx-target=");

    // 3. swap="..."
    out = out.replace(" swap=\"append\"", " hx-swap=\"beforeend\"");
    out = out.replace(" swap=\"prepend\"", " hx-swap=\"afterbegin\"");
    out = out.replace(" swap=\"", " hx-swap=\"");
    out = out.replace("\nswap=\"", "\nhx-swap=\"");
    out = out.replace("\tswap=\"", "\thx-swap=\"");
    out = out.replace(" swap='append'", " hx-swap='beforeend'");
    out = out.replace(" swap='prepend'", " hx-swap='afterbegin'");
    out = out.replace(" swap='", " hx-swap='");
    out = out.replace("\nswap='", "\nhx-swap='");
    out = out.replace("\tswap='", "\thx-swap='");
    out = out.replace(" swap=", " hx-swap=");
    out = out.replace("\nswap=", "\nhx-swap=");
    out = out.replace("\tswap=", "\thx-swap=");

    // 4. live="..." (e.g. live="300ms", live="blur")
    for prefix in &[" live=\"", "\nlive=\"", "\tlive=\""] {
        let mut start_idx = 0;
        while let Some(pos) = out[start_idx..].find(prefix) {
            let actual_pos = start_idx + pos;
            let after_prefix = actual_pos + prefix.len();
            if let Some(quote_end) = out[after_prefix..].find('"') {
                let val = &out[after_prefix..after_prefix + quote_end];
                let trigger_val = if val == "blur" || val == "change" {
                    val.to_string()
                } else if val.ends_with("ms") || val.ends_with('s') {
                    format!("keyup changed delay:{}", val)
                } else if val == "true" {
                    "keyup changed delay:300ms".to_string()
                } else {
                    val.to_string()
                };
                let full_span = actual_pos..after_prefix + quote_end + 1;
                let repl = format!(" hx-trigger=\"{}\"", trigger_val);
                out.replace_range(full_span, &repl);
                start_idx = actual_pos + repl.len();
            } else {
                start_idx = after_prefix;
            }
        }
    }
    out = out.replace(" live=", " hx-trigger=");
    out = out.replace("\nlive=", "\nhx-trigger=");

    out
}

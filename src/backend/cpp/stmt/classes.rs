use crate::backend::cpp::generator::CodeGenerator;
use crate::backend::cpp::stmt::types::*;
use crate::frontend::parser::ast::*;

impl CodeGenerator {
    pub(crate) fn visit_enum_decl(&mut self, decl: &Decl) {
        if let Decl::EnumDecl {
            name,
            generics,
            variants,
            ..
        } = decl {
                self.enum_types.insert(name.clone());
                let is_generic = !generics.is_empty();
                let has_payloads = variants
                    .iter()
                    .any(|v| !matches!(v.payload, EnumVariantPayload::None));

                if !is_generic && !has_payloads {
                    self.simple_enum_types.insert(name.clone());
                    self.emit(&format!("enum class {} {{", name));
                    self.indent_level += 1;
                    for (i, variant) in variants.iter().enumerate() {
                        let comma = if i < variants.len() - 1 { "," } else { "" };
                        self.emit(&format!("{}{}", variant.name, comma));
                    }
                    self.indent_level -= 1;
                    self.emit("};");

                    let is_used = self.used_enums.contains(name);
                    if is_used {
                        for variant in variants.iter() {
                            self.emit(&format!(
                                "inline constexpr auto {} = {}::{};",
                                variant.name, name, variant.name
                            ));
                        }
                    }

                    self.emit(&format!(
                        "inline fast_std::array<char> fastlang_as_str(const {}& obj) {{",
                        name
                    ));
                    self.indent_level += 1;
                    self.emit("switch (obj) {");
                    self.indent_level += 1;
                    for variant in variants.iter() {
                        self.emit(&format!(
                            "case {}::{}: return const_cast<char*>(\"{}\");",
                            name, variant.name, variant.name
                        ));
                    }
                    self.indent_level -= 1;
                    self.emit("}");
                    self.emit("return const_cast<char*>(\"\");");
                    self.indent_level -= 1;
                    self.emit("}");
                } else {
                    self.payload_enum_types.insert(name.clone());
                    let is_used = self.used_enums.contains(name);
                    for v in variants.iter() {
                        if matches!(v.payload, EnumVariantPayload::None) {
                            self.emit(&format!("struct fastlang_tag_{}_{} {{}};", name, v.name));
                            if is_used {
                                self.emit(&format!(
                                    "inline constexpr fastlang_tag_{}_{} {};",
                                    name, v.name, v.name
                                ));
                            }
                        }
                    }

                    let generic_params: Vec<String> = generics
                        .iter()
                        .map(|g| format!("typename {}", g.as_str()))
                        .collect();
                    let template_prefix = if is_generic {
                        format!("template <{}>\n", generic_params.join(", "))
                    } else {
                        String::new()
                    };

                    self.emit(&format!("{}struct {} {{", template_prefix, name));
                    self.indent_level += 1;

                    self.emit("enum class Tag {");
                    self.indent_level += 1;
                    for (i, v) in variants.iter().enumerate() {
                        let comma = if i < variants.len() - 1 { "," } else { "" };
                        self.emit(&format!("{}{}", v.name, comma));
                    }
                    self.indent_level -= 1;
                    self.emit("} tag;");

                    for v in variants.iter() {
                        let var_qualified = format!("{}::{}", name, v.name);
                        let var_methods = self.target_impl_methods.get(&var_qualified).cloned();
                        let var_handles = self.target_handle_methods.get(&var_qualified).cloned();

                        match &v.payload {
                            EnumVariantPayload::None => {
                                if var_methods.is_some() || var_handles.is_some() {
                                    self.emit(&format!("struct {}_Payload {{", v.name));
                                    self.indent_level += 1;
                                    if let Some(methods) = var_methods {
                                        let old_in_class = self.in_class_or_scope;
                                        self.in_class_or_scope = true;
                                        for m in &methods {
                                            self.visit_declaration(m);
                                        }
                                        self.in_class_or_scope = old_in_class;
                                    }
                                    if let Some(handles) = var_handles {
                                        self.emit_operator_overloads(&Some(handles));
                                    }
                                    self.indent_level -= 1;
                                    self.emit("};");
                                }
                            }
                            EnumVariantPayload::Tuple(types) => {
                                self.emit(&format!("struct {}_Payload {{", v.name));
                                self.indent_level += 1;
                                for (idx, t) in types.iter().enumerate() {
                                    self.emit(&format!("{} _{};", type_to_cpp(t), idx));
                                }
                                if types.len() == 1 {
                                    let t_str = type_to_cpp(&types[0]);
                                    if t_str.ends_with('*') {
                                        self.emit(&format!("operator {}() const {{ return _0; }}", t_str));
                                    } else {
                                        self.emit(&format!("operator {}&() {{ return _0; }}", t_str));
                                        self.emit(&format!(
                                            "operator const {}&() const {{ return _0; }}",
                                            t_str
                                        ));
                                    }
                                }
                                if let Some(methods) = var_methods {
                                    let old_in_class = self.in_class_or_scope;
                                    self.in_class_or_scope = true;
                                    for m in &methods {
                                        self.visit_declaration(m);
                                    }
                                    self.in_class_or_scope = old_in_class;
                                }
                                if let Some(handles) = var_handles {
                                    self.emit_operator_overloads(&Some(handles));
                                }
                                self.indent_level -= 1;
                                self.emit("};");
                            }
                            EnumVariantPayload::Struct(fields) => {
                                self.emit(&format!("struct {}_Payload {{", v.name));
                                self.indent_level += 1;
                                for f in fields {
                                    let type_str = type_to_cpp(&f.type_node);
                                    if let Some(ref def_val) = f.default_value {
                                        let def_code = self.visit_expression(def_val);
                                        self.emit(&format!(
                                            "{} {} = {};",
                                            type_str, f.name, def_code
                                        ));
                                    } else {
                                        self.emit(&format!("{} {};", type_str, f.name));
                                    }
                                }
                                if let Some(methods) = var_methods {
                                    let old_in_class = self.in_class_or_scope;
                                    self.in_class_or_scope = true;
                                    for m in &methods {
                                        self.visit_declaration(m);
                                    }
                                    self.in_class_or_scope = old_in_class;
                                }
                                if let Some(handles) = var_handles {
                                    self.emit_operator_overloads(&Some(handles));
                                }
                                self.indent_level -= 1;
                                self.emit("};");
                            }
                        }
                    }

                    self.emit("struct __no_init_t {};");
                    self.emit(&format!("{}(__no_init_t) {{}}", name));

                    self.emit("union Storage {");
                    self.indent_level += 1;
                    self.emit("char __dummy;");
                    for v in variants.iter() {
                        if !matches!(v.payload, EnumVariantPayload::None) {
                            self.emit(&format!("{}_Payload v_{};", v.name, v.name));
                        }
                    }
                    self.emit("Storage() : __dummy(0) {}");
                    self.emit("~Storage() {}");
                    self.indent_level -= 1;
                    self.emit("} storage;");

                    let payload_variants: Vec<_> = variants
                        .iter()
                        .filter(|v| !matches!(v.payload, EnumVariantPayload::None))
                        .collect();

                    self.emit("void __destroy() {");
                    self.indent_level += 1;
                    if !payload_variants.is_empty() {
                        self.emit("switch (tag) {");
                        self.indent_level += 1;
                        for v in &payload_variants {
                            self.emit(&format!(
                                "case Tag::{}: storage.v_{}.~{}_Payload(); break;",
                                v.name, v.name, v.name
                            ));
                        }
                        self.emit("default: break;");
                        self.indent_level -= 1;
                        self.emit("}");
                    }
                    self.indent_level -= 1;
                    self.emit("}");

                    self.emit(&format!("void __copy_from(const {}& other) {{", name));
                    self.indent_level += 1;
                    self.emit("tag = other.tag;");
                    if !payload_variants.is_empty() {
                        self.emit("switch (tag) {");
                        self.indent_level += 1;
                        for v in &payload_variants {
                            self.emit(&format!(
                                "case Tag::{}: new (&storage.v_{}) {}_Payload(other.storage.v_{}); break;",
                                v.name, v.name, v.name, v.name
                            ));
                        }
                        self.emit("default: break;");
                        self.indent_level -= 1;
                        self.emit("}");
                    }
                    self.indent_level -= 1;
                    self.emit("}");

                    self.emit(&format!("void __move_from({}&& other) {{", name));
                    self.indent_level += 1;
                    self.emit("tag = other.tag;");
                    if !payload_variants.is_empty() {
                        self.emit("switch (tag) {");
                        self.indent_level += 1;
                        for v in &payload_variants {
                            self.emit(&format!(
                                "case Tag::{}: new (&storage.v_{}) {}_Payload(std::move(other.storage.v_{})); break;",
                                v.name, v.name, v.name, v.name
                            ));
                        }
                        self.emit("default: break;");
                        self.indent_level -= 1;
                        self.emit("}");
                    }
                    self.indent_level -= 1;
                    self.emit("}");

                    self.emit(&format!(
                        "{}(const {}& other) {{ __copy_from(other); }}",
                        name, name
                    ));
                    self.emit(&format!(
                        "{}({}&& other) noexcept {{ __move_from(std::move(other)); }}",
                        name, name
                    ));
                    self.emit(&format!("{}& operator=(const {}& other) {{ if (this != &other) {{ __destroy(); __copy_from(other); }} return *this; }}", name, name));
                    self.emit(&format!("{}& operator=({}&& other) noexcept {{ if (this != &other) {{ __destroy(); __move_from(std::move(other)); }} return *this; }}", name, name));
                    self.emit(&format!("~{}() {{ __destroy(); }}", name));

                    for v in variants.iter() {
                        match &v.payload {
                            EnumVariantPayload::None => {
                                self.emit(&format!(
                                    "static {} {}() {{ {} r(__no_init_t{{}}); r.tag = Tag::{}; return r; }}",
                                    name, v.name, name, v.name
                                ));
                            }
                            EnumVariantPayload::Tuple(types) => {
                                let param_strs: Vec<String> = types
                                    .iter()
                                    .enumerate()
                                    .map(|(idx, t)| format!("{} _{}", type_to_cpp(t), idx))
                                    .collect();
                                let init_strs: Vec<String> = types
                                    .iter()
                                    .enumerate()
                                    .map(|(idx, _)| format!("_{}", idx))
                                    .collect();
                                self.emit(
                                    &format!(
                                        "static {} {}({}) {{ {} r(__no_init_t{{}}); r.tag = Tag::{}; new (&r.storage.v_{}) {}_Payload{{{}}}; return r; }}",
                                        name,
                                        v.name,
                                        param_strs.join(", "),
                                        name,
                                        v.name,
                                        v.name,
                                        v.name,
                                        init_strs.join(", ")
                                    )
                                );
                            }
                            EnumVariantPayload::Struct(fields) => {
                                let param_strs: Vec<String> = fields
                                    .iter()
                                    .map(|f| format!("{} {}", type_to_cpp(&f.type_node), f.name))
                                    .collect();
                                let init_strs: Vec<String> =
                                    fields.iter().map(|f| f.name.clone()).collect();
                                self.emit(
                                    &format!(
                                        "static {} {}({}) {{ {} r(__no_init_t{{}}); r.tag = Tag::{}; new (&r.storage.v_{}) {}_Payload{{{}}}; return r; }}",
                                        name,
                                        v.name,
                                        param_strs.join(", "),
                                        name,
                                        v.name,
                                        v.name,
                                        v.name,
                                        init_strs.join(", ")
                                    )
                                );
                            }
                        }
                    }

                    let default_variant = variants
                        .iter()
                        .find(|v| matches!(v.payload, EnumVariantPayload::None))
                        .unwrap_or(&variants[0]);
                    if matches!(default_variant.payload, EnumVariantPayload::None) {
                        self.emit(&format!(
                            "{}() : tag(Tag::{}) {{}}",
                            name, default_variant.name
                        ));
                    } else {
                        self.emit(&format!(
                            "{}() : tag(Tag::{}) {{ new (&storage.v_{}) {}_Payload(); }}",
                            name, default_variant.name, default_variant.name, default_variant.name
                        ));
                    }
                    for v in variants.iter() {
                        if matches!(v.payload, EnumVariantPayload::None) {
                            self.emit(&format!(
                                "{}(fastlang_tag_{}_{}) : tag(Tag::{}) {{}}",
                                name, name, v.name, v.name
                            ));
                            self.emit(
                                &format!(
                                    "{}& operator=(fastlang_tag_{}_{}) {{ __destroy(); tag = Tag::{}; return *this; }}",
                                    name,
                                    name,
                                    v.name,
                                    v.name
                                )
                            );
                            self.emit(
                                &format!(
                                    "bool operator==(fastlang_tag_{}_{}) const {{ return tag == Tag::{}; }}",
                                    name,
                                    v.name,
                                    v.name
                                )
                            );
                            self.emit(
                                &format!(
                                    "bool operator!=(fastlang_tag_{}_{}) const {{ return tag != Tag::{}; }}",
                                    name,
                                    v.name,
                                    v.name
                                )
                            );
                        }
                    }
                    self.emit("bool operator==(Tag t) const { return tag == t; }");
                    self.emit("bool operator!=(Tag t) const { return tag != t; }");
                    if payload_variants.is_empty() {
                        self.emit(&format!(
                            "bool operator==(const {}& other) const {{ return tag == other.tag; }}",
                            name
                        ));
                        self.emit(&format!(
                            "bool operator!=(const {}& other) const {{ return tag != other.tag; }}",
                            name
                        ));
                    }

                    if let Some(methods) = self.target_impl_methods.get(name).cloned() {
                        let old_in_class = self.in_class_or_scope;
                        self.in_class_or_scope = true;
                        for m in &methods {
                            self.visit_declaration(m);
                        }
                        self.in_class_or_scope = old_in_class;
                    }
                    if let Some(handles) = self.target_handle_methods.get(name).cloned() {
                        let old_in_class = self.in_class_or_scope;
                        self.in_class_or_scope = true;
                        for h in &handles {
                            self.visit_declaration(h);
                        }
                        self.in_class_or_scope = old_in_class;
                        self.emit_operator_overloads(&Some(handles));
                    }

                    self.indent_level -= 1;
                    self.emit("};");

                    fn type_contains_generic_name(t: &BaseType, g_name: &str) -> bool {
                        let clean_g = g_name.strip_prefix("...").unwrap_or(g_name);
                        if t.as_str() == clean_g
                            || t.get_name() == clean_g
                            || type_to_cpp(t) == clean_g
                        {
                            return true;
                        }
                        match t {
                            BaseType::GenericParam(name) | BaseType::New(name) => name == clean_g,
                            BaseType::Blueprint { name, .. }
                            | BaseType::Struct { name, .. }
                            | BaseType::Class { name, .. } => name == clean_g,
                            BaseType::Array { base_type, .. } => {
                                type_contains_generic_name(base_type, clean_g)
                            }
                            BaseType::RawPointer(inner)
                            | BaseType::Address(inner)
                            | BaseType::Type(inner) => type_contains_generic_name(inner, clean_g),
                            BaseType::Generic(args) => {
                                args.iter().any(|a| type_contains_generic_name(a, clean_g))
                            }
                            _ => false,
                        }
                    }

                    let is_used = self.used_enums.contains(name);
                    if is_used {
                        for v in variants.iter() {
                            let mut param_strs = Vec::new();
                            let mut arg_strs = Vec::new();
                            let mut member_decls = Vec::new();

                            match &v.payload {
                                EnumVariantPayload::None => continue,
                                EnumVariantPayload::Tuple(types) => {
                                    for (idx, t) in types.iter().enumerate() {
                                        param_strs.push(format!("{} _{}", type_to_cpp(t), idx));
                                        arg_strs.push(format!("_{}", idx));
                                        member_decls.push(format!("{} _{};", type_to_cpp(t), idx));
                                    }
                                }
                                EnumVariantPayload::Struct(fields) => {
                                    for f in fields {
                                        param_strs.push(format!(
                                            "{} {}",
                                            type_to_cpp(&f.type_node),
                                            f.name
                                        ));
                                        arg_strs.push(f.name.clone());
                                        member_decls.push(format!(
                                            "{} {};",
                                            type_to_cpp(&f.type_node),
                                            f.name
                                        ));
                                    }
                                }
                            }

                            if is_generic {
                                let template_params: Vec<String> = generics
                                    .iter()
                                    .map(|g| format!("typename {}", g.as_str()))
                                    .collect();
                                let gen_args: Vec<String> =
                                    generics.iter().map(|g| g.as_str()).collect();

                                let (used_generics, unused_generics): (
                                    Vec<&BaseType>,
                                    Vec<&BaseType>,
                                ) = generics.iter().partition(|g| {
                                    let g_name = if !g.get_name().is_empty() {
                                        g.get_name()
                                    } else {
                                        g.as_str()
                                    };
                                    let clean_g = g_name.strip_prefix("...").unwrap_or(&g_name);
                                    match &v.payload {
                                        EnumVariantPayload::Tuple(types) => types
                                            .iter()
                                            .any(|t| type_contains_generic_name(t, clean_g)),
                                        EnumVariantPayload::Struct(fields) => {
                                            fields.iter().any(|f| {
                                                type_contains_generic_name(&f.type_node, clean_g)
                                            })
                                        }
                                        EnumVariantPayload::None => false,
                                    }
                                });

                                if unused_generics.is_empty() {
                                    self.emit(&format!(
                                        "template <{}> inline {}<{}> {}({}) {{ return {}<{}>::{}({}); }}",
                                        template_params.join(", "),
                                        name,
                                        gen_args.join(", "),
                                        v.name,
                                        param_strs.join(", "),
                                        name,
                                        gen_args.join(", "),
                                        v.name,
                                        arg_strs.join(", ")
                                    ));
                                } else {
                                    let all_gen_args: Vec<String> =
                                        generics.iter().map(|g| g.as_str()).collect();
                                    if used_generics.is_empty() {
                                        let conv_template_params: Vec<String> = generics
                                            .iter()
                                            .map(|g| format!("typename {}", g.as_str()))
                                            .collect();
                                        self.emit(&format!(
                                            "struct fastlang_helper_{}_{} {{",
                                            name, v.name
                                        ));
                                        self.indent_level += 1;
                                        for decl in &member_decls {
                                            self.emit(decl);
                                        }
                                        self.emit(&format!(
                                            "template <{}>\noperator {}<{}>() const {{ return {}<{}>::{}({}); }}",
                                            conv_template_params.join(", "),
                                            name,
                                            all_gen_args.join(", "),
                                            name,
                                            all_gen_args.join(", "),
                                            v.name,
                                            arg_strs.join(", ")
                                        ));
                                        self.indent_level -= 1;
                                        self.emit("};");
                                        self.emit(&format!(
                                            "inline fastlang_helper_{}_{} {}({}) {{ return fastlang_helper_{}_{}{{{}}}; }}",
                                            name,
                                            v.name,
                                            v.name,
                                            param_strs.join(", "),
                                            name,
                                            v.name,
                                            arg_strs.join(", ")
                                        ));
                                    } else {
                                        let used_tmpl: Vec<String> = used_generics
                                            .iter()
                                            .map(|g| format!("typename {}", g.as_str()))
                                            .collect();
                                        let used_args: Vec<String> =
                                            used_generics.iter().map(|g| g.as_str()).collect();
                                        let unused_tmpl: Vec<String> = unused_generics
                                            .iter()
                                            .map(|g| format!("typename {}", g.as_str()))
                                            .collect();

                                        self.emit(&format!(
                                            "template <{}> struct fastlang_helper_{}_{} {{",
                                            used_tmpl.join(", "),
                                            name,
                                            v.name
                                        ));
                                        self.indent_level += 1;
                                        for decl in &member_decls {
                                            self.emit(decl);
                                        }
                                        self.emit(&format!(
                                            "template <{}>\noperator {}<{}>() const {{ return {}<{}>::{}({}); }}",
                                            unused_tmpl.join(", "),
                                            name,
                                            all_gen_args.join(", "),
                                            name,
                                            all_gen_args.join(", "),
                                            v.name,
                                            arg_strs.join(", ")
                                        ));
                                        self.indent_level -= 1;
                                        self.emit("};");
                                        self.emit(&format!(
                                            "template <{}> inline fastlang_helper_{}_{}<{}> {}({}) {{ return fastlang_helper_{}_{}<{}>{{{}}}; }}",
                                            used_tmpl.join(", "),
                                            name,
                                            v.name,
                                            used_args.join(", "),
                                            v.name,
                                            param_strs.join(", "),
                                            name,
                                            v.name,
                                            used_args.join(", "),
                                            arg_strs.join(", ")
                                        ));
                                    }
                                }
                            } else {
                                self.emit(&format!(
                                    "inline {} {}({}) {{ return {}::{}({}); }}",
                                    name,
                                    v.name,
                                    param_strs.join(", "),
                                    name,
                                    v.name,
                                    arg_strs.join(", ")
                                ));
                            }
                        }
                    }

                    let obj_type_str = if is_generic {
                        let generic_args: Vec<String> =
                            generics.iter().map(|g| g.as_str()).collect();
                        format!("{}<{}>", name, generic_args.join(", "))
                    } else {
                        name.clone()
                    };

                    let fn_decl_sig = if generics.is_empty() {
                        format!("template <typename _T = {}> inline fast_std::array<char> fastlang_as_str(const {}& obj) {{", name, name)
                    } else {
                        format!(
                            "{}inline fast_std::array<char> fastlang_as_str(const {}& obj) {{",
                            template_prefix, obj_type_str
                        )
                    };
                    self.emit(&fn_decl_sig);
                    self.indent_level += 1;
                    let target_t = if generics.is_empty() {
                        "_T"
                    } else {
                        &obj_type_str
                    };
                    self.emit(&format!(
                        "if constexpr (has_display<{}>::value) {{",
                        target_t
                    ));
                    self.indent_level += 1;
                    self.emit(&format!("return const_cast<{}&>(obj).display();", target_t));
                    self.indent_level -= 1;
                    self.emit("} else {");
                    self.indent_level += 1;
                    self.emit("switch (obj.tag) {");
                    self.indent_level += 1;
                    for v in variants.iter() {
                        self.emit(&format!(
                            "case {}::Tag::{}: return \"{}\";",
                            obj_type_str, v.name, v.name
                        ));
                    }
                    self.indent_level -= 1;
                    self.emit("}");
                    self.emit("return \"\";");
                    self.indent_level -= 1;
                    self.emit("}");
                    self.indent_level -= 1;
                    self.emit("}");
                }
            }

    }

    pub(crate) fn visit_class_decl(&mut self, decl: &Decl) {
        if let Decl::ClassDecl {
            name,
            extends,
            public_block,
            private_block,
            static_block,
            handle_block,
            constructor,
            generics,
            visibility: _,
            ..
        } = decl {
                let old_in_class = self.in_class_or_scope;
                let old_class_name = self.current_class_name.clone();
                self.in_class_or_scope = true;
                self.current_class_name = Some(name.clone());
                if !generics.is_empty() {
                    let gen_params: Vec<String> = generics
                        .iter()
                        .map(|g| format!("typename {}", g.as_str()))
                        .collect();
                    self.emit(&format!("template <{}>", gen_params.join(", ")));
                }
                let ext_code = if let Some(ext) = extends {
                    format!(": public {}", ext)
                } else {
                    "".to_string()
                };
                self.emit(&format!("class {} {} {{", name, ext_code));
                self.emit("public:");
                self.indent_level += 1;
                if decls_use_flag(public_block, &Flag::Broken)
                    || decls_use_flag(private_block, &Flag::Broken)
                    || decls_use_flag(handle_block, &Flag::Broken)
                {
                    self.emit("bool broken = false;");
                }
                if decls_use_flag(public_block, &Flag::IsDone)
                    || decls_use_flag(private_block, &Flag::IsDone)
                    || decls_use_flag(handle_block, &Flag::IsDone)
                {
                    self.emit("bool is_done = false;");
                }
                if decls_use_flag(public_block, &Flag::Yielded)
                    || decls_use_flag(private_block, &Flag::Yielded)
                    || decls_use_flag(handle_block, &Flag::Yielded)
                {
                    self.emit("bool yielded = false;");
                }
                if decls_use_flag(public_block, &Flag::Leaved)
                    || decls_use_flag(private_block, &Flag::Leaved)
                    || decls_use_flag(handle_block, &Flag::Leaved)
                {
                    self.emit("bool leaved = false;");
                }
                if decls_use_flag(public_block, &Flag::Returned)
                    || decls_use_flag(private_block, &Flag::Returned)
                    || decls_use_flag(handle_block, &Flag::Returned)
                {
                    self.emit("bool returned = false;");
                }
                if decls_use_flag(public_block, &Flag::Continued)
                    || decls_use_flag(private_block, &Flag::Continued)
                    || decls_use_flag(handle_block, &Flag::Continued)
                {
                    self.emit("bool continued = false;");
                }

                self.emit_operator_overloads(&Some(handle_block.clone()));

                for s in public_block {
                    self.visit_declaration(s);
                }
                for h in handle_block {
                    self.visit_declaration(h);
                }
                let has_drop_handle = handle_block.iter().any(|h| {
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
                    self.emit(&format!("~{}() {{ this->_fastlang_call_drop(); }}", name));
                    self.emit(&format!("{}(const {}&) = default;", name, name));
                    self.emit(&format!("{}& operator=(const {}&) = default;", name, name));
                    self.emit(&format!("{}({}&& _other) noexcept {{ *this = _other; _other._fastlang_dropped = true; }}", name, name));
                    self.emit(&format!("{}& operator=({}&& _other) noexcept {{ if (this != &_other) {{ this->_fastlang_call_drop(); *this = _other; _other._fastlang_dropped = true; this->_fastlang_dropped = false; }} return *this; }}", name, name));
                }
                for s in static_block {
                    self.visit_declaration(s);
                }

                let has_default_ctor = constructor
                    .as_ref()
                    .map(|ctors| {
                        ctors
                            .iter()
                            .any(|c| c.params.iter().all(|p| p.type_node.as_str() == "type"))
                    })
                    .unwrap_or(false);

                if let Some(ext) = extends {
                    self.emit(&format!("using {}::{};", ext, ext));
                }
                if !has_default_ctor {
                    self.emit(&format!("{}() {{}}", name));
                }
                if let Some(constructors) = constructor {
                    for c in constructors {
                        let param_list: Vec<String> = c
                            .params
                            .iter()
                            .filter(|p| p.type_node.as_str() != "type")
                            .map(|p| {
                                let cpp_t = type_to_cpp(&p.type_node);
                                if cpp_t == *name
                                    || cpp_t.ends_with(&format!("::{}", name))
                                    || p.type_node.get_name() == *name
                                {
                                    if let Some(ref def_val) = p.default_value {
                                        let def_code = self.visit_expression(def_val);
                                        format!("const {}& {} = {}", cpp_t, p.name, def_code)
                                    } else {
                                        format!("const {}& {}", cpp_t, p.name)
                                    }
                                } else {
                                    self.format_param(p)
                                }
                            })
                            .collect();
                        self.emit(&format!("{}({}) {{", name, param_list.join(", ")));
                        self.indent_level += 1;
                        for s in &c.body {
                            self.visit_statement(s);
                        }
                        self.indent_level -= 1;
                        self.emit("}");

                        if c.params.len() == 1
                            && matches!(&c.params[0].type_node, BaseType::Array { base_type, .. } if matches!(**base_type, BaseType::Char))
                        {
                            self.emit(
                                &format!(
                                    "template <size_t N> {}(const char (&arr)[N]) : {}(fastlang_string_create_from_ptr(arr)) {{}}",
                                    name,
                                    name
                                )
                            );
                            self.emit(&format!(
                                "{}(const char* s) : {}(fastlang_string_create_from_ptr(s)) {{}}",
                                name, name
                            ));
                        }
                    }
                }

                self.indent_level -= 1;
                if !private_block.is_empty() {
                    self.emit("private:");
                    self.indent_level += 1;
                    for s in private_block {
                        self.visit_declaration(s);
                    }
                    self.indent_level -= 1;
                }
                self.emit("};");
                self.in_class_or_scope = old_in_class;
                self.current_class_name = old_class_name;
            }

    }

    pub(crate) fn visit_struct_decl(&mut self, decl: &Decl) {
        if let Decl::StructDecl {
            name,
            public_block,
            private_block,
            static_block,
            constructor,
            handle_block,
            visibility: _,
            ..
        } = decl {
                let old_in_class = self.in_class_or_scope;
                let old_class_name = self.current_class_name.clone();
                self.in_class_or_scope = true;
                self.current_class_name = Some(name.clone());
                self.emit(&format!("struct {} {{", name));
                self.indent_level += 1;
                if decls_use_flag(public_block, &Flag::Broken)
                    || decls_use_flag(private_block, &Flag::Broken)
                    || decls_use_flag(static_block, &Flag::Broken)
                {
                    self.emit("bool broken = false;");
                }
                if decls_use_flag(public_block, &Flag::IsDone)
                    || decls_use_flag(private_block, &Flag::IsDone)
                    || decls_use_flag(static_block, &Flag::IsDone)
                {
                    self.emit("bool is_done = false;");
                }
                if decls_use_flag(public_block, &Flag::Yielded)
                    || decls_use_flag(private_block, &Flag::Yielded)
                    || decls_use_flag(static_block, &Flag::Yielded)
                {
                    self.emit("bool yielded = false;");
                }
                if decls_use_flag(public_block, &Flag::Leaved)
                    || decls_use_flag(private_block, &Flag::Leaved)
                    || decls_use_flag(static_block, &Flag::Leaved)
                {
                    self.emit("bool leaved = false;");
                }
                if decls_use_flag(public_block, &Flag::Returned)
                    || decls_use_flag(private_block, &Flag::Returned)
                    || decls_use_flag(static_block, &Flag::Returned)
                {
                    self.emit("bool returned = false;");
                }
                if decls_use_flag(public_block, &Flag::Continued)
                    || decls_use_flag(private_block, &Flag::Continued)
                    || decls_use_flag(static_block, &Flag::Continued)
                {
                    self.emit("bool continued = false;");
                }
                for s in public_block {
                    self.visit_declaration(s);
                }
                for s in static_block {
                    self.visit_declaration(s);
                }
                if let Some(constructors) = constructor {
                    for c in constructors {
                        let param_list: Vec<String> = c
                            .params
                            .iter()
                            .filter(|p| p.type_node.as_str() != "type")
                            .map(|p| {
                                let cpp_t = type_to_cpp(&p.type_node);
                                if cpp_t == *name
                                    || cpp_t.ends_with(&format!("::{}", name))
                                    || p.type_node.get_name() == *name
                                {
                                    format!("const {}& {}", cpp_t, p.name)
                                } else {
                                    format!("{} {}", cpp_t, p.name)
                                }
                            })
                            .collect();
                        self.emit(&format!("{}({}) {{", name, param_list.join(", ")));
                        self.indent_level += 1;
                        for s in &c.body {
                            self.visit_statement(s);
                        }
                        self.indent_level -= 1;
                        self.emit("}");
                    }
                }

                self.emit_operator_overloads(&Some(handle_block.clone()));

                for h in handle_block {
                    self.visit_declaration(h);
                }
                let has_drop_handle = handle_block.iter().any(|h| {
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
                    self.emit(&format!("~{}() {{ this->_fastlang_call_drop(); }}", name));
                    self.emit(&format!("{}() = default;", name));
                    self.emit(&format!("{}(const {}&) = default;", name, name));
                    self.emit(&format!("{}& operator=(const {}&) = default;", name, name));
                    self.emit(&format!("{}({}&& _other) noexcept {{ *this = _other; _other._fastlang_dropped = true; }}", name, name));
                    self.emit(&format!("{}& operator=({}&& _other) noexcept {{ if (this != &_other) {{ this->_fastlang_call_drop(); *this = _other; _other._fastlang_dropped = true; this->_fastlang_dropped = false; }} return *this; }}", name, name));
                }
                self.indent_level -= 1;
                if !private_block.is_empty() {
                    self.emit("private:");
                    self.indent_level += 1;
                    for s in private_block {
                        self.visit_declaration(s);
                    }
                    self.indent_level -= 1;
                }
                self.emit("};");
                self.in_class_or_scope = old_in_class;
                self.current_class_name = old_class_name;
            }

    }
}

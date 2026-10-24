use super::*;

impl SemanticAnalyzer {
    pub(crate) fn resolve_type(&self, ty: &BaseType) -> BaseType {
        match ty {
            BaseType::Blueprint { name, .. }
            | BaseType::Struct { name, .. }
            | BaseType::Class { name, .. }
            | BaseType::Enum { name, .. } => {
                if let Some(aliased) = self.current_env.borrow().resolve_type_alias(name) {
                    self.resolve_type(&aliased)
                } else {
                    ty.clone()
                }
            }
            BaseType::Array { base_type, size } => BaseType::Array {
                base_type: Box::new(self.resolve_type(base_type)),
                size: size.clone(),
            },
            BaseType::Generic(vec) => {
                BaseType::Generic(vec.iter().map(|t| self.resolve_type(t)).collect())
            }
            _ => ty.clone(),
        }
    }

    pub(crate) fn is_known_type(&self, type_node: &BaseType) -> bool {
        match type_node {
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
            | BaseType::Void
            | BaseType::Auto
            | BaseType::Number
            | BaseType::Unknown
            | BaseType::GenericParam(_)
            | BaseType::Identifier
            | BaseType::Expr => true,
            BaseType::Array { base_type, .. } => self.is_known_type(base_type),
            BaseType::RawPointer(inner) | BaseType::Address(inner) => self.is_known_type(inner),
            BaseType::Fn { return_type, params, .. }
            | BaseType::Lambda { return_type, params, .. }
            | BaseType::Method { return_type, params, .. }
            | BaseType::Function { return_type, params, .. }
            | BaseType::Micro { return_type, params, .. }
            | BaseType::Macro { return_type, params, .. } => {
                self.is_known_type(return_type) && params.iter().all(|p| self.is_known_type(p))
            }
            _ => {
                let s = type_node.as_str();
                let clean = BaseType::from_str(&s)
                    .blueprint_name()
                    .map(|b| b.to_string())
                    .unwrap_or_else(|| s.clone());
                let base_name = clean.split('<').next().unwrap_or(&clean).trim();
                let base_name = base_name.trim_end_matches('*').trim_end_matches('&').trim();

                if self.is_primitive_stack_type(base_name) || self.is_generic_type(base_name) {
                    return true;
                }
                if self.current_env.borrow().lookup_blueprint(base_name).is_some() {
                    return true;
                }
                if self.global_metadata.contains_key(base_name) {
                    return true;
                }
                if let Some(info) = self.current_env.borrow().lookup(base_name) {
                    if let SymbolKind::Variable { type_node, .. } = &info.kind {
                        if matches!(type_node, BaseType::Type(_)) {
                            return true;
                        }
                    }
                }
                let std_qual = format!("std::{}", base_name);
                if self.global_metadata.contains_key(&std_qual)
                    || self.current_env.borrow().lookup_blueprint(&std_qual).is_some()
                {
                    return true;
                }
                let types_qual = format!("std/types::{}", base_name);
                if self.global_metadata.contains_key(&types_qual)
                    || self.current_env.borrow().lookup_blueprint(&types_qual).is_some()
                {
                    return true;
                }
                let coll_qual = format!("std/collections::{}", base_name);
                if self.global_metadata.contains_key(&coll_qual)
                    || self.current_env.borrow().lookup_blueprint(&coll_qual).is_some()
                {
                    return true;
                }
                let err_qual = format!("std/error::{}", base_name);
                if self.global_metadata.contains_key(&err_qual)
                    || self.current_env.borrow().lookup_blueprint(&err_qual).is_some()
                {
                    return true;
                }
                false
            }
        }
    }

    pub(crate) fn is_primitive_stack_type(&self, type_name: &str) -> bool {
        matches!(
            type_name,
            "int"
                | "int8"
                | "int16"
                | "int32"
                | "int64"
                | "int128"
                | "uint"
                | "uint8"
                | "uint16"
                | "uint32"
                | "uint64"
                | "uint128"
                | "usize"
                | "isize"
                | "float"
                | "float32"
                | "float64"
                | "float128"
                | "char"
                | "uchar"
                | "str"
                | "ustr"
                | "bool"
                | "void"
                | "type"
                | "any"
                | "unknown"
                | "number"
                | "num"
        )
    }

    pub(crate) fn type_has_drop_handle(&self, type_name: &str) -> bool {
        let extracted = BaseType::from_str(type_name)
            .blueprint_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| type_name.to_string());
        let clean_name = if let Some(idx) = extracted.find('<') {
            extracted[..idx].trim().to_string()
        } else {
            extracted
        };
        if let Some(bp) = self.current_env.borrow().lookup_blueprint(&clean_name) {
            if bp.handles.contains(&HandleMethods::Drop)
                || bp.handle_signatures.contains_key("drop")
                || bp.handle_signatures.contains_key("$drop")
            {
                return true;
            }
        }
        if let Some(meta) = self.global_metadata.get(&clean_name) {
            if meta.handles.contains(&HandleMethods::Drop)
                || meta.handle_signatures.contains_key("drop")
                || meta.handle_signatures.contains_key("$drop")
            {
                return true;
            }
        }
        false
    }

    pub(crate) fn is_class_type(&self, type_name: &str) -> bool {
        let clean_name = BaseType::from_str(type_name)
            .blueprint_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| type_name.to_string());
        if let Some(meta) = self.global_metadata.get(&clean_name) {
            if let BaseType::Class { .. } = &meta.ty {
                return true;
            }
        }
        false
    }

    pub(crate) fn is_struct_or_blueprint_type(&self, type_name: &str) -> bool {
        let clean_name = BaseType::from_str(type_name)
            .blueprint_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| type_name.to_string());
        if let Some(meta) = self.global_metadata.get(&clean_name) {
            if matches!(
                &meta.ty,
                BaseType::Struct { .. } | BaseType::Blueprint { .. }
            ) {
                return true;
            }
        }
        if self
            .current_env
            .borrow()
            .lookup_blueprint(&clean_name)
            .is_some()
        {
            return true;
        }
        false
    }
    pub(crate) fn is_subclass_of(&self, derived: &str, base: &str) -> bool {
        let mut curr = derived;
        while let Some(parent) = self.class_hierarchy.get(curr) {
            if parent == base {
                return true;
            }
            curr = parent;
        }
        false
    }

    pub(crate) fn types_are_compatible_basetype(
        &self,
        expected: &BaseType,
        actual: &BaseType,
    ) -> bool {
        let res_exp = self.resolve_type(expected);
        let res_act = self.resolve_type(actual);
        let expected = &res_exp;
        let actual = &res_act;

        if expected == actual {
            return true;
        }

        if matches!((expected, actual), (BaseType::Identifier, BaseType::Identifier) | (BaseType::Expr, BaseType::Expr)) {
            return true;
        }
        if matches!(expected, BaseType::Identifier) && matches!(actual, BaseType::Str | BaseType::Array { .. } | BaseType::Identifier) {
            return true;
        }
        if matches!(expected, BaseType::Expr) {
            return true;
        }

        // Generic type handling (e.g. T, U, or GenericParam)
        if matches!(expected, BaseType::GenericParam(_))
            || matches!(actual, BaseType::GenericParam(_))
        {
            return true;
        }
        if let Some(n) = expected.type_name() {
            if self.is_generic_type(n) {
                return true;
            }
        }
        if let Some(n) = actual.type_name() {
            if self.is_generic_type(n) {
                return true;
            }
        }

        // Auto, Unknown, Any handling
        match (expected, actual) {
            (BaseType::Auto, _) | (_, BaseType::Auto) => return true,
            (BaseType::Unknown, _) | (_, BaseType::Unknown) => return true,
            _ => {}
        }

        // Flag and Bool interchangeability
        if matches!(
            (expected, actual),
            (BaseType::Flag, BaseType::Bool) | (BaseType::Bool, BaseType::Flag)
        ) {
            return true;
        }

        // Object handling: Object accepts any Class, Struct, Blueprint, Enum
        if matches!(expected, BaseType::Object) {
            if matches!(
                actual,
                BaseType::Object
                    | BaseType::Class { .. }
                    | BaseType::Struct { .. }
                    | BaseType::Blueprint { .. }
                    | BaseType::Enum { .. }
            ) {
                return true;
            }
            if let Some(name) = actual.type_name() {
                if self.is_class_type(name) || self.is_struct_or_blueprint_type(name) {
                    return true;
                }
            }
        }
        if matches!(actual, BaseType::Object) {
            if matches!(
                expected,
                BaseType::Class { .. }
                    | BaseType::Struct { .. }
                    | BaseType::Blueprint { .. }
                    | BaseType::Enum { .. }
            ) {
                return true;
            }
            if let Some(name) = expected.type_name() {
                if self.is_class_type(name) || self.is_struct_or_blueprint_type(name) {
                    return true;
                }
            }
        }

        // Number handling: BaseType::Number accepts any numeric type
        if matches!(expected, BaseType::Number) && actual.is_numeric() {
            return true;
        }
        if matches!(actual, BaseType::Number) && expected.is_numeric() {
            return true;
        }

        // Numeric widening
        if is_numeric_widening_basetype(expected, actual) {
            return true;
        }

        // Integer families (int8..int128, uint8..uint128, usize, isize)
        if expected.is_integer() && actual.is_integer() {
            return true;
        }

        // Float families (float32, float64, float128)
        if expected.is_float() && actual.is_float() {
            return true;
        }

        // Type expressions: Type(T) accepts Type, Int, UInt, Bool, Float
        if let BaseType::Type(_) = expected {
            if matches!(actual, BaseType::Type(_))
                || actual.is_numeric()
                || matches!(actual, BaseType::Bool)
            {
                return true;
            }
        }

        // Functions / Fn / Lambda / Method
        if expected.is_callable() && actual.is_callable() {
            return true;
        }

        // Array compatibility
        if let (
            BaseType::Array {
                base_type: exp_inner,
                ..
            },
            BaseType::Array {
                base_type: act_inner,
                ..
            },
        ) = (expected, actual)
        {
            return self.types_are_compatible_basetype(exp_inner, act_inner);
        }

        // str and ustr compatibility
        if matches!(
            (expected, actual),
            (BaseType::Str, BaseType::Str) | (BaseType::UStr, BaseType::UStr)
        ) {
            return true;
        }
        if matches!((expected, actual), (BaseType::Str, BaseType::Array { ref base_type, .. }) if **base_type == BaseType::Char || matches!(**base_type, BaseType::GenericParam(_)) || self.is_generic_type(&base_type.as_str()) || self.scope.in_generic_template)
        {
            return true;
        }
        if matches!((expected, actual), (BaseType::Array { ref base_type, .. }, BaseType::Str) if **base_type == BaseType::Char || matches!(**base_type, BaseType::GenericParam(_)) || self.is_generic_type(&base_type.as_str()) || self.scope.in_generic_template)
        {
            return true;
        }
        if matches!((expected, actual), (BaseType::UStr, BaseType::Array { ref base_type, .. }) if **base_type == BaseType::UChar || matches!(**base_type, BaseType::GenericParam(_)) || self.is_generic_type(&base_type.as_str()) || self.scope.in_generic_template)
        {
            return true;
        }
        if matches!((expected, actual), (BaseType::Array { ref base_type, .. }, BaseType::UStr) if **base_type == BaseType::UChar || matches!(**base_type, BaseType::GenericParam(_)) || self.is_generic_type(&base_type.as_str()) || self.scope.in_generic_template)
        {
            return true;
        }

        // Address / Pointer compatibility
        if let BaseType::Address(exp_inner) = expected {
            if matches!(**exp_inner, BaseType::Unknown) {
                return true;
            }
            let act_target = match actual {
                BaseType::Address(inner) => inner.as_ref(),
                BaseType::Array { base_type, .. } => base_type.as_ref(),
                BaseType::RawPointer(inner) => inner.as_ref(),
                other => other,
            };
            return self.types_are_compatible_basetype(exp_inner, act_target);
        }
        if let BaseType::Address(act_inner) = actual {
            return self.types_are_compatible_basetype(expected, act_inner);
        }

        // RawPointer compatibility: raw_ptr<T> accepts raw_ptr<T>, array<T>, or address<T>
        if let BaseType::RawPointer(exp_inner) = expected {
            if matches!(**exp_inner, BaseType::Unknown) {
                return true;
            }
            let mut act_target = match actual {
                BaseType::RawPointer(inner) => inner.as_ref(),
                BaseType::Array { base_type, .. } => base_type.as_ref(),
                BaseType::Address(inner) => inner.as_ref(),
                other => other,
            };
            while let BaseType::Array { base_type, .. } = act_target {
                act_target = base_type.as_ref();
            }
            return self.types_are_compatible_basetype(exp_inner, act_target);
        }
        if let BaseType::RawPointer(act_inner) = actual {
            return self.types_are_compatible_basetype(expected, act_inner);
        }

        // Lambda and Function compatibility
        if let (
            BaseType::Fn {
                params: exp_p,
                return_type: exp_r,
                ..
            }
            | BaseType::Lambda {
                params: exp_p,
                return_type: exp_r,
                ..
            }
            | BaseType::Method {
                params: exp_p,
                return_type: exp_r,
                ..
            },
            BaseType::Fn {
                params: act_p,
                return_type: act_r,
                ..
            }
            | BaseType::Lambda {
                params: act_p,
                return_type: act_r,
                ..
            }
            | BaseType::Method {
                params: act_p,
                return_type: act_r,
                ..
            },
        ) = (expected, actual)
        {
            if exp_p.len() != act_p.len() {
                return false;
            }
            let params_compat = exp_p
                .iter()
                .zip(act_p.iter())
                .all(|(p1, p2)| self.types_are_compatible_basetype(p1, p2));
            let ret_compat = self.types_are_compatible_basetype(exp_r, act_r);
            return params_compat && ret_compat;
        }

        // Generic param checks
        if let BaseType::GenericParam(_) = expected {
            return true;
        }
        if let BaseType::GenericParam(_) = actual {
            return true;
        }

        // Blueprint / Class / Struct handle / constructor checks
        let exp_name = expected.type_name();
        let act_name = actual.type_name();

        if let Some(exp_n) = exp_name {
            if let Some(bp) = self
                .current_env
                .borrow()
                .lookup_blueprint(exp_n)
                .or_else(|| {
                    self.global_metadata
                        .get(exp_n)
                        .map(build_blueprint_from_metadata)
                })
            {
                let actual_str = actual.as_str();
                if bp.handle_accepts_type(HandleMethods::Equal, &actual_str)
                    || bp.handle_accepts_type(HandleMethods::EqualAssign, &actual_str)
                    || bp.handle_accepts_type(HandleMethods::ArrowAssign, &actual_str)
                    || bp.handle_accepts_type(HandleMethods::Arrow, &actual_str)
                {
                    return true;
                }
                if let Some(meta) = self.global_metadata.get(exp_n) {
                    if let Some(constructors) = &meta.constructor {
                        for c in constructors {
                            if c.params.len() == 1 {
                                let param_t = &c.params[0].type_node;
                                if param_t == actual
                                    || (param_t != expected
                                        && self.types_are_compatible_basetype(param_t, actual))
                                {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }

            if let Some(sym) = self.current_env.borrow().lookup(exp_n) {
                if let SymbolKind::Variable {
                    type_node: BaseType::Type(inner),
                    ..
                } = &sym.kind
                {
                    if matches!(**inner, BaseType::GenericParam(_)) {
                        return true;
                    }
                    if inner.as_ref() != expected {
                        return self.types_are_compatible_basetype(inner, actual);
                    }
                }
            }
        }

        if let Some(act_n) = act_name {
            if let Some(sym) = self.current_env.borrow().lookup(act_n) {
                if let SymbolKind::Variable {
                    type_node: BaseType::Type(inner),
                    ..
                } = &sym.kind
                {
                    if matches!(**inner, BaseType::GenericParam(_)) {
                        return true;
                    }
                    if inner.as_ref() != actual {
                        return self.types_are_compatible_basetype(expected, inner);
                    }
                }
            }
        }

        // Class inheritance check
        if let (Some(exp_n), Some(act_n)) = (exp_name, act_name) {
            if self.is_subclass_of(act_n, exp_n) {
                return true;
            }
        }

        false
    }

    pub(crate) fn types_are_compatible(&self, expected: &str, actual: &str) -> bool {
        if expected == actual
            || expected == "any"
            || expected == "unknown"
            || actual == "unknown"
            || expected == "auto"
            || actual == "auto"
            || actual == "undefined"
            || expected.starts_with("...")
            || actual.starts_with("...")
        {
            return true;
        }
        if (expected == "str" || expected == "array<char>")
            && (actual == "str" || actual == "array<char>" || actual == "array<T>" || actual == "T[]" || self.is_generic_type(actual) || (self.scope.in_generic_template && actual.starts_with("array<")))
        {
            return true;
        }
        if (expected == "ident" && (actual == "str" || actual == "array<char>" || actual == "ident"))
            || (actual == "ident" && (expected == "str" || expected == "array<char>" || expected == "ident"))
        {
            return true;
        }
        if expected == "expr" || actual == "expr" {
            return true;
        }
        if (actual == "str" || actual == "array<char>")
            && (expected == "str" || expected == "array<char>" || expected == "array<T>" || expected == "T[]" || self.is_generic_type(expected) || (self.scope.in_generic_template && expected.starts_with("array<")))
        {
            return true;
        }
        let exp_ty = self.resolve_type(&BaseType::from_str(expected));
        let act_ty = self.resolve_type(&BaseType::from_str(actual));
        self.types_are_compatible_basetype(&exp_ty, &act_ty)
    }

    // ----------------------------------------------------------
    // Helpers
    // ----------------------------------------------------------

    pub(crate) fn is_primitive_numeric(t: &str) -> bool {
        matches!(
            t,
            "int"
                | "int8"
                | "int16"
                | "int32"
                | "int64"
                | "int128"
                | "uint"
                | "uint8"
                | "uint16"
                | "uint32"
                | "uint64"
                | "uint128"
                | "byte"
                | "usize"
                | "isize"
                | "float"
                | "float32"
                | "float64"
                | "float128"
        )
    }

    pub fn get_fast_type(&self, inner_type: &str) -> crate::middle_end::interpreter::FastType {
        let base_name = if inner_type.contains('<') {
            inner_type.split('<').next().unwrap_or(inner_type)
        } else {
            inner_type
        };
        let (
            has_display,
            has_copy,
            has_cast,
            has_throw,
            has_default,
            has_share,
            handles,
            methods,
            fields,
            field_types,
            method_types,
            handle_types,
        ) = if let Some(bp) = self.current_env.borrow().lookup_blueprint(base_name) {
            let mut ft_map = std::collections::HashMap::new();
            for (k, v) in &bp.fields {
                ft_map.insert(k.clone(), v.as_str().to_string());
            }
            let mut mt_map = std::collections::HashMap::new();
            for (k, v) in &bp.methods {
                mt_map.insert(k.clone(), v.return_type.as_str().to_string());
            }
            let mut ht_map = std::collections::HashMap::new();
            for (k, v) in &bp.handle_signatures {
                ht_map.insert(k.clone(), v.return_type.as_str().to_string());
            }
            (
                bp.handles.iter().any(|h| h.as_str() == "display"),
                bp.handles
                    .iter()
                    .any(|h| h.as_str() == "copy" || matches!(h, HandleMethods::Copy)),
                bp.handles
                    .iter()
                    .any(|h| h.as_str() == "cast" || matches!(h, HandleMethods::Cast)),
                bp.handles.iter().any(|h| {
                    h.as_str() == "throw"
                        || h.as_str() == "$throw"
                        || matches!(h, HandleMethods::Throw)
                }),
                bp.handles
                    .iter()
                    .any(|h| h.as_str() == "default" || matches!(h, HandleMethods::Default)),
                bp.handles
                    .iter()
                    .any(|h| h.as_str() == "share" || matches!(h, HandleMethods::Share)),
                bp.handles.iter().map(|h| h.as_str().to_string()).collect(),
                bp.methods.keys().cloned().collect(),
                bp.fields.keys().cloned().collect(),
                ft_map,
                mt_map,
                ht_map,
            )
        } else {
            (
                false,
                false,
                false,
                false,
                false,
                false,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                std::collections::HashMap::new(),
                std::collections::HashMap::new(),
                std::collections::HashMap::new(),
            )
        };

        crate::middle_end::interpreter::FastType::with_full_metadata(
            crate::frontend::parser::ast::BaseType::from_str(inner_type),
            has_display,
            has_copy,
            has_cast,
            has_throw,
            has_default,
            has_share,
            handles,
            methods,
            fields,
            field_types,
            method_types,
            handle_types,
        )
    }

    pub fn is_generic_type(&self, type_str: &str) -> bool {
        let mut clean = type_str.trim().trim_start_matches("...");
        while (clean.starts_with("type<") && clean.ends_with('>'))
            || (clean.starts_with("array<") && clean.ends_with('>'))
            || clean.ends_with("[]")
        {
            if clean.ends_with("[]") {
                clean = clean.trim_end_matches("[]");
            } else {
                let open_idx = match clean.find('<') {
                    Some(i) => i,
                    None => break,
                };
                clean = &clean[open_idx + 1..clean.len() - 1];
            }
            clean = clean.trim().trim_start_matches("...");
        }
        for prefix in &["blueprint::", "struct::", "class::", "enum::"] {
            if let Some(rest) = clean.strip_prefix(prefix) {
                clean = rest;
                break;
            }
        }

        // 1. Check if defined in current environment as a generic parameter symbol
        let check_env = |name: &str| -> bool {
            if let Some(info) = self.current_env.borrow().lookup(name) {
                if let SymbolKind::Variable { type_node, .. } = &info.kind {
                    if matches!(type_node, BaseType::GenericParam(_)) {
                        return true;
                    }
                    if let BaseType::Type(inner) = type_node {
                        if matches!(**inner, BaseType::GenericParam(_)) {
                            return true;
                        }
                    }
                }
            }
            false
        };

        if check_env(clean) || check_env(&format!("...{}", clean)) {
            return true;
        }

        // 2. An unbound type is only generic/unresolved if we are within a generic template context
        if self.scope.in_generic_template
            && !self.is_primitive_stack_type(clean)
            && !self.global_metadata.contains_key(clean)
            && self.current_env.borrow().lookup_blueprint(clean).is_none()
        {
            return true;
        }

        false
    }

    pub fn eval_type_reflection_bool(&self, inner_type: &str, prop: &str) -> Option<bool> {
        if self.is_generic_type(inner_type) {
            return None;
        }
        let ft = self.get_fast_type(inner_type);
        if let Some(method_name) = prop.strip_prefix("has_") {
            return Some(ft.has_method(method_name) || ft.has_handle(method_name));
        }
        match prop {
            "printable" => Some(ft.printable()),
            "pointer" => Some(ft.is_pointer()),
            "array" => Some(ft.is_array()),
            "primitive" => Some(ft.is_primitive()),
            "throwable" => Some(ft.throwable()),
            "shareable" => Some(ft.shareable()),
            "copyable" => Some(ft.copyable()),
            "castable" => Some(ft.castable()),
            "constant" => Some(ft.is_constant()),
            "runtime" => Some(ft.is_runtime()),
            "comptime" => Some(ft.is_comptime()),
            "undefined" => Some(ft.is_undefined()),
            _ => None,
        }
    }

    pub fn eval_static_bool_expr(&mut self, expr: &Expr) -> Option<bool> {
        match expr {
            Expr::LiteralBool(b) => Some(*b),
            Expr::UnaryOp { operator, operand } if operator == "!" => {
                self.eval_static_bool_expr(operand).map(|b| !b)
            }
            Expr::BinaryOp {
                left,
                operator,
                right,
            } => {
                if operator == "&&" {
                    let l = self.eval_static_bool_expr(left);
                    let r = self.eval_static_bool_expr(right);
                    if l == Some(false) || r == Some(false) {
                        return Some(false);
                    }
                    if let (Some(lb), Some(rb)) = (l, r) {
                        return Some(lb && rb);
                    }
                    return None;
                }
                if operator == "||" {
                    let l = self.eval_static_bool_expr(left);
                    let r = self.eval_static_bool_expr(right);
                    if l == Some(true) || r == Some(true) {
                        return Some(true);
                    }
                    if let (Some(lb), Some(rb)) = (l, r) {
                        return Some(lb || rb);
                    }
                    return None;
                }
                if operator == "==" || operator == "!=" {
                    let l_b = self.eval_static_bool_expr(left);
                    let r_b = self.eval_static_bool_expr(right);
                    if let (Some(lb), Some(rb)) = (l_b, r_b) {
                        let eq = lb == rb;
                        return Some(if operator == "==" { eq } else { !eq });
                    }

                    let resolve_static_sym =
                        |analyzer: &SemanticAnalyzer, e: &Expr| -> Option<String> {
                            match e {
                                Expr::Call { callee, .. } => {
                                    if let Expr::NamespaceAccess {
                                        namespace,
                                        property,
                                    } = &**callee
                                    {
                                        if namespace == "@compile" {
                                            if let Expr::Identifier(p) = &**property {
                                                if p == "backend" {
                                                    let variant =
                                                        if analyzer.target_backend == "cranelift" {
                                                            "Cranelift"
                                                        } else {
                                                            "Cpp"
                                                        };
                                                    return Some(format!("Backend::{}", variant));
                                                } else if p == "system" {
                                                    let variant = if cfg!(target_os = "windows") {
                                                        "Windows"
                                                    } else if cfg!(target_os = "macos") {
                                                        "MacOS"
                                                    } else {
                                                        "Linux"
                                                    };
                                                    return Some(format!("System::{}", variant));
                                                }
                                            }
                                        }
                                    }
                                    None
                                }
                                Expr::NamespaceAccess {
                                    namespace,
                                    property,
                                } => {
                                    if namespace == "@compile" {
                                        if let Expr::Identifier(p) = &**property {
                                            if p == "backend" {
                                                let variant =
                                                    if analyzer.target_backend == "cranelift" {
                                                        "Cranelift"
                                                    } else {
                                                        "Cpp"
                                                    };
                                                return Some(format!("Backend::{}", variant));
                                            } else if p == "system" {
                                                let variant = if cfg!(target_os = "windows") {
                                                    "Windows"
                                                } else if cfg!(target_os = "macos") {
                                                    "MacOS"
                                                } else {
                                                    "Linux"
                                                };
                                                return Some(format!("System::{}", variant));
                                            }
                                        }
                                    } else if let Expr::Identifier(p) = &**property {
                                        return Some(format!("{}::{}", namespace, p));
                                    }
                                    None
                                }
                                Expr::LiteralString(s) => Some(s.clone()),
                                Expr::Identifier(id) => Some(id.clone()),
                                _ => None,
                            }
                        };

                    if let (Some(s1), Some(s2)) = (
                        resolve_static_sym(self, left),
                        resolve_static_sym(self, right),
                    ) {
                        let eq = s1 == s2;
                        return Some(if operator == "==" { eq } else { !eq });
                    }
                }
                None
            }
            Expr::Call { callee, args, .. } => {
                if let Expr::PropertyAccess { object, property } = &**callee {
                    let inner_opt = if let Ok(obj_type) = self.visit_expression(object) {
                        if obj_type.starts_with("type<") {
                            Some(strip_wrapper(&obj_type, "type<").to_string())
                        } else if obj_type == "type" {
                            if let Expr::Call {
                                callee: inner_callee,
                                args: inner_args,
                                ..
                            } = &**object
                            {
                                let is_typeof = match &**inner_callee {
                                    Expr::Identifier(name) => name == "typeof",
                                    Expr::NamespaceAccess {
                                        namespace,
                                        property,
                                    } => {
                                        namespace == "@compile"
                                            && matches!(&**property, Expr::Identifier(p) if p == "typeof")
                                    }
                                    _ => false,
                                };
                                if is_typeof {
                                    if let Some(arg) = inner_args.first() {
                                        self.visit_expression(arg).ok()
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(inner) = inner_opt {
                        if property == "castable_to" {
                            if let Some(target_expr) = args.first() {
                                if let Ok(target_type_str) = self.visit_expression(target_expr) {
                                    let target_inner = strip_wrapper(&target_type_str, "type<");
                                    if self.is_generic_type(&inner)
                                        || self.is_generic_type(target_inner)
                                    {
                                        return None;
                                    }
                                    let target_ft = self.get_fast_type(target_inner);
                                    let ft = self.get_fast_type(&inner);
                                    if target_ft.base_type == BaseType::Unknown
                                        || ft.base_type == BaseType::Unknown
                                    {
                                        return None;
                                    }
                                    return Some(ft.castable_to(&target_ft));
                                }
                            }
                        }
                        let extract_name_or_type = |e: &Expr| -> String {
                            match e {
                                Expr::LiteralString(s) => strip_wrapper(s, "type<").to_string(),
                                Expr::Identifier(id) => strip_wrapper(id, "type<").to_string(),
                                Expr::PropertyAccess { property, .. } => property.clone(),
                                _ => format!("{:?}", e),
                            }
                        };
                        if property == "has_handle" {
                            if let Some(arg) = args.first() {
                                let s1 = extract_name_or_type(arg);
                                let s2 = args.get(1).map(extract_name_or_type);
                                let ft = self.get_fast_type(&inner);
                                return Some(ft.has_handle_query(&s1, s2.as_deref()));
                            }
                        }
                        if property == "has_method" {
                            if let Some(arg) = args.first() {
                                let s1 = extract_name_or_type(arg);
                                let s2 = args.get(1).map(extract_name_or_type);
                                let ft = self.get_fast_type(&inner);
                                return Some(ft.has_method_query(&s1, s2.as_deref()));
                            }
                        }
                        if property == "has_field" {
                            if let Some(arg) = args.first() {
                                let s1 = extract_name_or_type(arg);
                                let s2 = args.get(1).map(extract_name_or_type);
                                let ft = self.get_fast_type(&inner);
                                return Some(ft.has_field_query(&s1, s2.as_deref()));
                            }
                        }
                        return self.eval_type_reflection_bool(&inner, property);
                    }
                }
                None
            }
            Expr::PropertyAccess { object, property } => {
                if let Ok(obj_type) = self.visit_expression(object) {
                    if obj_type.starts_with("type<") {
                        let inner = strip_wrapper(&obj_type, "type<");
                        return self.eval_type_reflection_bool(inner, property);
                    } else if obj_type == "type" {
                        if let Expr::Call {
                            callee: inner_callee,
                            args,
                            ..
                        } = &**object
                        {
                            let is_typeof = match &**inner_callee {
                                Expr::Identifier(name) => name == "typeof",
                                Expr::NamespaceAccess {
                                    namespace,
                                    property,
                                } => {
                                    namespace == "@compile"
                                        && matches!(&**property, Expr::Identifier(p) if p == "typeof")
                                }
                                _ => false,
                            };
                            if is_typeof {
                                if let Some(arg) = args.first() {
                                    if let Ok(t) = self.visit_expression(arg) {
                                        return self.eval_type_reflection_bool(&t, property);
                                    }
                                }
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }
}

pub(crate) fn strip_wrapper<'a>(s: &'a str, prefix: &str) -> &'a str {
    if let Some(rest) = s.strip_prefix(prefix) {
        if let Some(inner) = rest.strip_suffix('>') {
            return inner;
        }
        return rest;
    }
    s
}

pub(crate) fn extract_type_args_from_str(s: &str) -> (String, Vec<BaseType>) {
    let mut trimmed = s.trim();
    for prefix in &[
        "blueprint::",
        "struct::",
        "class::",
        "enum::",
        "machine::",
        "block::",
        "object::",
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            trimmed = rest.trim();
            break;
        }
    }

    if let Some(start) = trimmed.find('<') {
        if let Some(rest) = trimmed.strip_suffix('>') {
            let base = trimmed[..start].trim().to_string();
            let inside = &rest[start + 1..];
            let mut args = Vec::new();
            for arg_str in inside.split(',') {
                let clean_arg = arg_str.trim();
                if !clean_arg.is_empty() {
                    args.push(BaseType::from_str(clean_arg));
                }
            }
            return (base, args);
        }
    }
    (trimmed.to_string(), Vec::new())
}

#[allow(dead_code)]
pub(crate) fn extract_iter_payload_type(t: &BaseType) -> String {
    match t {
        BaseType::Enum { name, generics, .. } if name == "Option" && !generics.is_empty() => {
            extract_iter_payload_type(&generics[0])
        }
        BaseType::Generic(vec) if !vec.is_empty() => extract_iter_payload_type(&vec[0]),
        _ => t.as_str(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumericKind {
    Signed,
    Unsigned,
    Float,
}

pub(crate) fn numeric_rank_basetype(ty: &BaseType) -> Option<(NumericKind, u32)> {
    match ty {
        BaseType::Int(size) => {
            let bits = match size {
                Size::S8 => 8,
                Size::S16 => 16,
                Size::S32 => 32,
                Size::S64 => 64,
                Size::S128 => 128,
            };
            Some((NumericKind::Signed, bits))
        }
        BaseType::ISize => Some((NumericKind::Signed, 64)),
        BaseType::UInt(size) => {
            let bits = match size {
                Size::S8 => 8,
                Size::S16 => 16,
                Size::S32 => 32,
                Size::S64 => 64,
                Size::S128 => 128,
            };
            Some((NumericKind::Unsigned, bits))
        }
        BaseType::USize => Some((NumericKind::Unsigned, 64)),
        BaseType::Float(size) => {
            let bits = match size {
                Size::S8 | Size::S16 | Size::S32 => 32,
                Size::S64 => 64,
                Size::S128 => 128,
            };
            Some((NumericKind::Float, bits))
        }
        _ => None,
    }
}

pub(crate) fn is_numeric_widening_basetype(expected: &BaseType, actual: &BaseType) -> bool {
    let (exp_kind, exp_rank) = match numeric_rank_basetype(expected) {
        Some(r) => r,
        None => return false,
    };
    let (act_kind, act_rank) = match numeric_rank_basetype(actual) {
        Some(r) => r,
        None => return false,
    };

    if exp_kind == act_kind {
        return exp_rank >= act_rank;
    }
    if exp_kind == NumericKind::Float {
        return exp_rank >= act_rank;
    }
    if exp_kind == NumericKind::Signed && act_kind == NumericKind::Unsigned {
        return exp_rank > act_rank;
    }
    false
}

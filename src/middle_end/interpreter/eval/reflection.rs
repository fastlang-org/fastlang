use super::*;
use std::collections::HashMap;
use crate::frontend::parser::ast::*;
use crate::middle_end::interpreter::fast_type::FastType;
use crate::middle_end::semantic::environment::SymbolKind;

impl Interpreter {
    pub fn get_fast_type(&self, name: &str) -> FastType {
        let clean = strip_type_wrapper(name);
        let base_name = if clean.contains('<') {
            clean.split('<').next().unwrap_or(clean)
        } else if clean.ends_with("[]") {
            "array"
        } else {
            clean
        };
        if let Some(ref env_rc) = self.current_env {
            if let Some(bp) = env_rc.borrow().lookup_blueprint(base_name) {
                let has_display = bp.handles.iter().any(|h| h.as_str() == "display");
                let has_copy = bp
                    .handles
                    .iter()
                    .any(|h| h.as_str() == "copy" || matches!(h, HandleMethods::Copy));
                let has_cast = bp
                    .handles
                    .iter()
                    .any(|h| h.as_str() == "cast" || matches!(h, HandleMethods::Cast));
                let has_throw = bp.handles.iter().any(|h| {
                    h.as_str() == "throw"
                        || h.as_str() == "$throw"
                        || matches!(h, HandleMethods::Throw)
                }) || bp.handle_signatures.contains_key("throw")
                    || bp.handle_signatures.contains_key("$throw");
                let has_default = bp
                    .handles
                    .iter()
                    .any(|h| h.as_str() == "default" || matches!(h, HandleMethods::Default));
                let has_share = bp
                    .handles
                    .iter()
                    .any(|h| h.as_str() == "share" || matches!(h, HandleMethods::Share));
                let handles: Vec<String> =
                    bp.handles.iter().map(|h| h.as_str().to_string()).collect();
                let methods: Vec<String> = bp.methods.keys().cloned().collect();
                let fields: Vec<String> = bp.fields.keys().cloned().collect();
                let mut field_types = HashMap::new();
                for (fname, fty) in &bp.fields {
                    field_types.insert(fname.clone(), fty.as_str().to_string());
                }
                let mut method_types = HashMap::new();
                for (mname, msig) in &bp.methods {
                    method_types.insert(mname.clone(), msig.return_type.as_str().to_string());
                }
                let mut handle_types = HashMap::new();
                for (hname, hsig) in &bp.handle_signatures {
                    handle_types.insert(hname.clone(), hsig.return_type.as_str().to_string());
                }
                return FastType::with_full_metadata(
                    BaseType::from_str(clean),
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
                );
            }
        }
        FastType::from_name(clean)
    }

    pub fn extract_type_or_name(&self, expr: &Expr) -> String {
        match expr {
            Expr::LiteralString(s) => strip_type_wrapper(s).to_string(),
            Expr::Identifier(id) => {
                if let Ok(Expr::LiteralString(s)) = self.env.get(id) {
                    strip_type_wrapper(&s).to_string()
                } else {
                    strip_type_wrapper(id).to_string()
                }
            }
            Expr::PropertyAccess { property, .. } => property.clone(),
            _ => self.format_expr_for_print(expr),
        }
    }

    pub fn default_for_type(&self, name: &str) -> Option<Expr> {
        let clean = strip_type_wrapper(name);
        match clean {
            "int8" | "int16" | "int32" | "int64" | "int" | "uint8" | "uint16" | "uint32"
            | "uint64" | "uint" | "usize" | "isize" => Some(Expr::LiteralInt(0)),
            "float32" | "float64" | "float" => Some(Expr::LiteralFloat(0.0)),
            "bool" => Some(Expr::LiteralBool(false)),
            "char" => Some(Expr::LiteralChar('\0')),
            "uchar" => Some(Expr::LiteralUChar(0)),
            _ => {
                let base_name = if clean.contains('<') {
                    clean.split('<').next().unwrap_or(clean)
                } else {
                    clean
                };
                if let Some(ref env_rc) = self.current_env {
                    if let Some(bp) = env_rc.borrow().lookup_blueprint(base_name) {
                        let has_default = bp.handles.iter().any(|h| {
                            h.as_str() == "default" || matches!(h, HandleMethods::Default)
                        });
                        if has_default || bp.is_class {
                            return Some(Expr::Default(Some(BaseType::from_str(clean))));
                        }
                    }
                }
                None
            }
        }
    }

    pub fn resolve_type_str(&self, ty: &str) -> String {
        let mut current = ty.to_string();
        for _ in 0..10 {
            if let Some(ref env_rc) = self.current_env {
                if let Some(info) = env_rc.borrow().lookup(&current) {
                    if let SymbolKind::Variable {
                        type_node: BaseType::Type(inner),
                        ..
                    } = &info.kind {
                        current = inner.as_str();
                        continue;
                    }
                }
            }
            break;
        }
        current
    }
}

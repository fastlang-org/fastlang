use crate::frontend::parser::ast::{self, HandleMethods};
use crate::middle_end::semantic::environment::{BlueprintData, Environment, FnSignature};
use std::cell::RefCell;
use std::rc::Rc;

// ─────────────────────────────────────────────────────────────────────────────
// op_to_handle — maps an operator string to its corresponding HandleMethods
// ─────────────────────────────────────────────────────────────────────────────
pub fn op_to_handle(op: &str) -> HandleMethods {
    match op {
        "->" => HandleMethods::Arrow,
        "arrow_assign" => HandleMethods::ArrowAssign,
        "=>" => HandleMethods::FatArrow,
        "=" => HandleMethods::Equal,
        "+=" => HandleMethods::AddReassign,
        "-=" => HandleMethods::SubReassign,
        "*=" => HandleMethods::MulReassign,
        "/=" => HandleMethods::DivReassign,
        "%=" => HandleMethods::ModReassign,
        "+" => HandleMethods::Add,
        "-" => HandleMethods::Sub,
        "*" => HandleMethods::Mul,
        "/" => HandleMethods::Div,
        "%" => HandleMethods::Mod,
        "==" => HandleMethods::PartialEqual,
        "!=" => HandleMethods::NotEqual,
        ">" => HandleMethods::GreaterThan,
        "<" => HandleMethods::LessThan,
        ">=" => HandleMethods::GreaterThanEqual,
        "<=" => HandleMethods::LessThanEqual,
        "&&" => HandleMethods::And,
        "||" => HandleMethods::Or,
        "++" => HandleMethods::Increment,
        "--" => HandleMethods::Decrement,
        "." => HandleMethods::PropertyAccess,
        "::" => HandleMethods::NamespaceAccess,
        "-." => HandleMethods::HandleAccess,
        _ => HandleMethods::NotFound,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// resolve_handle_for_op — looks up a blueprint defining a handle for the given operator
// Used in VarDecl and ReassignStmt to verify operator overloading
// ─────────────────────────────────────────────────────────────────────────────
pub fn resolve_handle_for_op(
    env: &Rc<RefCell<Environment>>,
    blueprint_name: &str,
    op: &str,
) -> HandleLookupResult {
    let handle = op_to_handle(op);

    if matches!(handle, HandleMethods::NotFound) {
        return HandleLookupResult::UnknownOp;
    }

    let bp = env.borrow().lookup_blueprint(blueprint_name);
    match bp {
        None => HandleLookupResult::BlueprintNotFound,
        Some(data) => {
            if data.has_handle(handle) {
                HandleLookupResult::Found(data)
            } else {
                let fallback = match handle {
                    HandleMethods::AddReassign => Some(HandleMethods::Add),
                    HandleMethods::SubReassign => Some(HandleMethods::Sub),
                    HandleMethods::MulReassign => Some(HandleMethods::Mul),
                    HandleMethods::DivReassign => Some(HandleMethods::Div),
                    HandleMethods::ModReassign => Some(HandleMethods::Mod),
                    _ => None,
                };
                if let Some(fb) = fallback {
                    if data.has_handle(fb) {
                        return HandleLookupResult::Found(data);
                    }
                }
                HandleLookupResult::HandleMissing { handle }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// HandleLookupResult — outcome of handle resolution
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum HandleLookupResult {
    /// Handle was found in the blueprint
    Found(BlueprintData),
    /// Blueprint was not found in the scope (not yet defined)
    BlueprintNotFound,
    /// Blueprint exists but does not implement this handle
    HandleMissing { handle: HandleMethods },
    /// Operator is unknown
    UnknownOp,
}

// ─────────────────────────────────────────────────────────────────────────────
// build_blueprint_from_metadata
// Converts TypeMetadata (from parser) into BlueprintData
// ─────────────────────────────────────────────────────────────────────────────
// build_blueprint_from_base_type — creates a BlueprintData from BaseType
// ─────────────────────────────────────────────────────────────────────────────
pub fn build_blueprint_from_base_type(
    ty: &crate::frontend::parser::ast::BaseType,
) -> BlueprintData {
    use crate::frontend::parser::ast::BaseType;
    use crate::middle_end::semantic::environment::FnSignature;

    let name = ty.get_name();
    let mut bp = BlueprintData::new(name);

    match ty {
        BaseType::Struct {
            fields,
            methods,
            generics,
            ..
        }
        | BaseType::Blueprint {
            fields,
            methods,
            generics,
            ..
        } => {
            for (f_name, f_type) in fields.as_ref() {
                bp.fields.insert(f_name.clone(), f_type.clone());
            }
            for (m_name, fn_type) in methods.as_ref() {
                let sig = FnSignature {
                    name: fn_type.name.clone(),
                    generics: fn_type.generics.clone(),
                    params: fn_type.params.clone(),
                    return_type: fn_type.return_type.clone(),
                    is_virtual: false,
                    is_abstract: false,
                };
                bp.methods.insert(m_name.clone(), sig);
            }
            for g in generics {
                if let BaseType::New(name) = g {
                    bp.generics.push(name.clone());
                } else {
                    let s = g.as_str();
                    if !s.is_empty() && s != "unknown" {
                        bp.generics.push(s);
                    }
                }
            }
        }
        BaseType::Class {
            fields,
            methods,
            generics,
            ..
        } => {
            bp.is_class = true;
            for (f_name, f_type) in fields.as_ref() {
                bp.fields.insert(f_name.clone(), f_type.clone());
            }
            for (m_name, fn_type) in methods.as_ref() {
                let sig = FnSignature {
                    name: fn_type.name.clone(),
                    generics: fn_type.generics.clone(),
                    params: fn_type.params.clone(),
                    return_type: fn_type.return_type.clone(),
                    is_virtual: true,
                    is_abstract: false,
                };
                bp.methods.insert(m_name.clone(), sig);
            }
            for g in generics {
                if let BaseType::New(name) = g {
                    bp.generics.push(name.clone());
                } else {
                    let s = g.as_str();
                    if !s.is_empty() && s != "unknown" {
                        bp.generics.push(s);
                    }
                }
            }
        }
        BaseType::Enum {
            variants,
            methods,
            generics,
            ..
        } => {
            for v in variants {
                bp.fields.insert(v.name.clone(), ty.clone());
            }
            for (m_name, fn_type) in methods.as_ref() {
                let hk = HandleMethods::from_str(m_name.as_str());
                let sig = FnSignature {
                    name: fn_type.name.clone(),
                    generics: fn_type.generics.clone(),
                    params: fn_type.params.clone(),
                    return_type: fn_type.return_type.clone(),
                    is_virtual: false,
                    is_abstract: false,
                };
                if hk != HandleMethods::NotFound {
                    bp.define_handle(hk, sig);
                } else {
                    bp.methods.insert(m_name.clone(), sig);
                }
            }
            for g in generics {
                if let BaseType::New(name) = g {
                    bp.generics.push(name.clone());
                } else {
                    let s = g.as_str();
                    if !s.is_empty() && s != "unknown" {
                        bp.generics.push(s);
                    }
                }
            }
        }
        BaseType::Machine {
            fields, methods, ..
        } => {
            for (f_name, f_type) in fields.as_ref() {
                bp.fields.insert(f_name.clone(), f_type.clone());
            }
            for (m_name, fn_type) in methods.as_ref() {
                let hk = HandleMethods::from_str(m_name.as_str());
                let sig = FnSignature {
                    name: fn_type.name.clone(),
                    generics: fn_type.generics.clone(),
                    params: fn_type.params.clone(),
                    return_type: fn_type.return_type.clone(),
                    is_virtual: false,
                    is_abstract: false,
                };
                if hk != HandleMethods::NotFound {
                    bp.define_handle(hk, sig);
                } else {
                    bp.methods.insert(m_name.clone(), sig);
                }
            }
        }
        BaseType::Block { methods, .. } => {
            for (m_name, fn_type) in methods.as_ref() {
                let hk = HandleMethods::from_str(m_name.as_str());
                let sig = FnSignature {
                    name: fn_type.name.clone(),
                    generics: fn_type.generics.clone(),
                    params: fn_type.params.clone(),
                    return_type: fn_type.return_type.clone(),
                    is_virtual: false,
                    is_abstract: false,
                };
                if hk != HandleMethods::NotFound {
                    bp.define_handle(hk, sig);
                } else {
                    bp.methods.insert(m_name.clone(), sig);
                }
            }
        }
        _ => {}
    }

    bp
}

pub fn build_blueprint_from_metadata(meta: &ast::TypeMetadata) -> BlueprintData {
    let mut bp = build_blueprint_from_base_type(&meta.ty);
    for h in &meta.handles {
        bp.handles.insert(*h);
    }
    for (f_name, f_type) in &meta.fields {
        bp.fields
            .entry(f_name.clone())
            .or_insert_with(|| f_type.clone());
    }
    let is_class = bp.is_class;
    for (m_name, fn_type) in &meta.methods {
        let sig = FnSignature {
            name: fn_type.name.clone(),
            generics: fn_type.generics.clone(),
            params: fn_type.params.clone(),
            return_type: fn_type.return_type.clone(),
            is_virtual: is_class,
            is_abstract: false,
        };
        bp.methods.entry(m_name.clone()).or_insert_with(|| sig);
    }
    for (h_name, fn_type) in &meta.handle_signatures {
        let hk = HandleMethods::from_str(h_name.as_str());
        let sig = FnSignature {
            name: fn_type.name.clone(),
            generics: fn_type.generics.clone(),
            params: fn_type.params.clone(),
            return_type: fn_type.return_type.clone(),
            is_virtual: is_class,
            is_abstract: false,
        };
        if hk != HandleMethods::NotFound {
            bp.define_handle(hk, sig);
        } else {
            bp.handle_signatures
                .entry(h_name.clone())
                .or_insert_with(|| sig);
        }
    }
    bp
}

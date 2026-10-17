pub mod blueprint;
pub mod functions;
pub mod preregister;
pub mod types;
pub mod vars;

pub use preregister::make_sig_key;

use super::*;

impl SemanticAnalyzer {
    pub(crate) fn make_blueprint_symbol(&self, name: &str, visibility: Visibility) -> SymbolInfo {
        SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Blueprint,
            visibility,
            dependencies: vec![],
            is_used: false,
            is_param: false,
            is_uninitialized: false,
            is_compilable: false,
        }
    }

    pub(crate) fn visit_declaration(&mut self, decl: &Decl) -> Result<(), String> {
        match decl {
            Decl::VarDecl {
                visibility,
                editability,
                type_node,
                place: _,
                name,
                value,
                assign_op,
            } => {
                self.analyze_var_decl(visibility, editability, type_node, name, value, assign_op)
            }

            Decl::DestructureDecl {
                visibility,
                editability,
                type_node,
                assignments,
                assign_op,
                ..
            } => {
                self.visit_destructure_decl(visibility, editability, type_node, assignments, assign_op)
            }

            Decl::ObjectDestructureDecl {
                visibility,
                editability,
                type_name,
                fields,
                rhs,
                ..
            } => {
                self.visit_object_destructure_decl(visibility, editability, type_name, fields, rhs)
            }

            Decl::ArrayDecl {
                visibility,
                editability,
                type_node,
                name,
                length,
                value,
                ..
            } => {
                self.visit_array_decl(visibility, editability, type_node, name, length, value)
            }

            Decl::ClassDecl {
                visibility,
                name,
                extends,
                handles,
                public_block,
                private_block,
                static_block,
                generics,
                handle_block,
                constructor,
            } => {
                self.visit_class_decl(
                    visibility,
                    name,
                    extends.as_deref(),
                    handles,
                    public_block,
                    private_block,
                    static_block,
                    generics,
                    handle_block,
                    constructor,
                )
            }

            Decl::StructDecl {
                visibility,
                name,
                handles,
                public_block,
                private_block,
                handle_block,
                static_block,
                constructor,
            } => {
                self.visit_struct_decl(
                    visibility,
                    name,
                    handles,
                    public_block,
                    private_block,
                    static_block,
                    &[],
                    handle_block,
                    constructor,
                )
            }

            Decl::EnumDecl {
                visibility,
                name,
                handle_block,
                variants,
                ..
            } => {
                self.visit_enum_decl(visibility, name, handle_block, variants)
            }

            Decl::BlockDecl {
                visibility,
                name,
                return_type,
                statements,
            } => {
                self.visit_block_decl(visibility, name, return_type, statements)
            }

            Decl::MachineDecl {
                visibility,
                name,
                labels,
                handle_block,
                ..
            } => {
                self.visit_machine_decl(visibility, name, labels, handle_block)
            }

            Decl::FnDecl {
                visibility,
                name,
                params,
                return_type,
                body,
                is_virtual,
                is_abstract,
                generics,
                ..
            } => {
                self.visit_fn_decl(
                    visibility,
                    name,
                    params,
                    return_type,
                    body,
                    *is_virtual,
                    *is_abstract,
                    generics,
                )
            }

            Decl::MicroDecl {
                visibility,
                name,
                params,
                return_type,
                body,
                generics,
            } => {
                self.visit_micro_decl(visibility, name, params, return_type, body, generics)
            }

            Decl::LabelDecl { name, body, .. } => {
                self.visit_label_decl(name, body)
            }

            Decl::BlueprintDecl {
                name,
                visibility,
                definition,
                share_directive,
                ..
            } => {
                self.visit_blueprint_decl(name, visibility, definition, share_directive)
            }

            Decl::ImplDecl {
                target,
                target_generics,
                is_handle_impl,
                methods,
                handle_block,
            } => {
                self.visit_impl_decl(target, target_generics, *is_handle_impl, methods, handle_block)
            }

            Decl::ExternFnDecl {
                name,
                params,
                return_type,
                alias,
                ..
            } => {
                self.visit_extern_fn_decl(name, params, return_type, alias)
            }

            Decl::ExternBlockDecl { decls, .. } => {
                self.visit_extern_block_decl(decls)
            }

            Decl::DefineDecl {
                name,
                type_alias,
                value,
                visibility,
            } => {
                self.visit_define_decl(name, type_alias, value, visibility)
            }

            Decl::NamespaceDecl { name, decls, .. } => {
                self.visit_namespace_decl(name, decls)
            }

            Decl::CompileDecl { name, decls } => {
                self.visit_compile_decl(name, decls)
            }

            Decl::MacroDecl {
                name,
                params,
                body,
                visibility,
                ..
            } => {
                self.visit_macro_decl(name, params, body, visibility)
            }

            Decl::Import { .. } => Ok(()),
            _ => Ok(()),
        }
    }
}

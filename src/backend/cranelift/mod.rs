use crate::backend::cranelift::context::{is_current_block_terminated, FunctionContext};
use crate::backend::cranelift::stmt::lower_stmt;
use crate::backend::cranelift::types::map_base_type;
use crate::frontend::parser::ast::*;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};
use std::collections::HashMap;

pub mod aot;
pub mod context;
pub mod expr;
pub mod stmt;
pub mod syscalls;
pub mod types;

pub struct CraneliftBackend {
    module: JITModule,
    builder_context: FunctionBuilderContext,
    ctx: codegen::Context,
    funcs: HashMap<String, cranelift_module::FuncId>,
}

impl Default for CraneliftBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CraneliftBackend {
    pub fn new() -> Self {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();

        let isa_builder = cranelift_native::builder().unwrap_or_else(|msg| {
            panic!("host machine is not supported: {}", msg);
        });
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();

        let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        let module = JITModule::new(builder);
        let ctx = module.make_context();

        Self {
            module,
            builder_context: FunctionBuilderContext::new(),
            ctx,
            funcs: HashMap::new(),
        }
    }

    /// Compile program AST directly into Cranelift JIT
    pub fn compile_ast(&mut self, ast: &[Stmt]) {
        let default_conv = self.module.target_config().default_call_conv;

        // 1. Declare functions
        for stmt in ast {
            if let Stmt::Declaration(Decl::FnDecl {
                name,
                params,
                return_type,
                generics,
                is_abstract,
                ..
            }) = stmt
            {
                if !generics.is_empty() || *is_abstract || self.funcs.contains_key(name) {
                    continue;
                }
                let mut sig = self.module.make_signature();
                sig.call_conv = default_conv;
                for param in params {
                    sig.params
                        .push(cranelift::prelude::AbiParam::new(map_base_type(
                            &param.type_node,
                        )));
                }
                if *return_type != BaseType::Void {
                    sig.returns
                        .push(cranelift::prelude::AbiParam::new(map_base_type(
                            return_type,
                        )));
                }
                let func_id = self
                    .module
                    .declare_function(name, Linkage::Export, &sig)
                    .unwrap();
                self.funcs.insert(name.clone(), func_id);
            }
        }

        // 2. Define functions
        let funcs_clone = self.funcs.clone();
        for stmt in ast {
            if let Stmt::Declaration(Decl::FnDecl {
                name,
                params,
                return_type,
                generics,
                is_abstract,
                body,
                ..
            }) = stmt
            {
                if !generics.is_empty() || *is_abstract {
                    continue;
                }
                if let Some(&func_id) = funcs_clone.get(name) {
                    self.ctx.clear();
                    self.builder_context = FunctionBuilderContext::new();
                    self.ctx.func.signature.call_conv = default_conv;

                    for param in params {
                        self.ctx
                            .func
                            .signature
                            .params
                            .push(cranelift::prelude::AbiParam::new(map_base_type(
                                &param.type_node,
                            )));
                    }
                    if *return_type != BaseType::Void {
                        self.ctx
                            .func
                            .signature
                            .returns
                            .push(cranelift::prelude::AbiParam::new(map_base_type(
                                return_type,
                            )));
                    }

                    let mut builder = cranelift_frontend::FunctionBuilder::new(
                        &mut self.ctx.func,
                        &mut self.builder_context,
                    );

                    let entry_block = builder.create_block();
                    builder.append_block_params_for_function_params(entry_block);
                    builder.switch_to_block(entry_block);

                    let ret_ty = if *return_type != BaseType::Void {
                        Some(map_base_type(return_type))
                    } else {
                        None
                    };
                    let mut fn_ctx = FunctionContext::new(funcs_clone.clone(), ret_ty);
                    let block_params = builder.block_params(entry_block).to_vec();
                    for (i, param) in params.iter().enumerate() {
                        let param_ty = map_base_type(&param.type_node);
                        let var = fn_ctx.declare_variable(&mut builder, &param.name, param_ty);
                        builder.def_var(var, block_params[i]);
                    }

                    for s in body {
                        lower_stmt(&mut self.module, &mut builder, &mut fn_ctx, s);
                    }

                    if !is_current_block_terminated(&builder) {
                        if *return_type == BaseType::Void {
                            builder.ins().return_(&[]);
                        } else {
                            let zero = builder.ins().iconst(map_base_type(return_type), 0);
                            builder.ins().return_(&[zero]);
                        }
                    }

                    builder.seal_block(entry_block);
                    let target_config = self.module.target_config();
                    builder.finalize(target_config);

                    let _ = self.module.define_function(func_id, &mut self.ctx);
                }
            }
        }
    }

    pub fn finalize(mut self) {
        self.module.finalize_definitions().unwrap();
        println!("CRANELIFT: JIT Module Finalized.");
    }
}

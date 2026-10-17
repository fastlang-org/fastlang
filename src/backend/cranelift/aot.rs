use crate::frontend::parser::ast::*;
use cranelift::prelude::*;
use cranelift_module::{Linkage, Module};
use cranelift_object::ObjectModule;
use std::collections::HashMap;

pub struct CraneliftAotBackend {
    module: ObjectModule,
    builder_context: FunctionBuilderContext,
    ctx: codegen::Context,
    funcs: HashMap<String, cranelift_module::FuncId>,
}

impl Default for CraneliftAotBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CraneliftAotBackend {
    pub fn new() -> Self {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();

        let isa_builder = cranelift_native::builder().unwrap_or_else(|msg| {
            panic!("host machine is not supported: {}", msg);
        });
        let isa = isa_builder.finish(settings::Flags::new(flag_builder)).unwrap();

        let builder = cranelift_object::ObjectBuilder::new(
            isa,
            "fast_lang_module",
            cranelift_module::default_libcall_names(),
        )
        .unwrap();
        let module = ObjectModule::new(builder);
        let ctx = module.make_context();

        Self {
            module,
            builder_context: FunctionBuilderContext::new(),
            ctx,
            funcs: HashMap::new(),
        }
    }

    /// Direct AST compilation for AOT backend
    pub fn compile_ast(&mut self, ast: &[Stmt]) {
        let default_conv = self.module.target_config().default_call_conv;

        // 1. Declare functions
        for stmt in ast {
            if let Stmt::Declaration(Decl::FnDecl { name, params, return_type, .. }) = stmt {
                if self.funcs.contains_key(name) {
                    continue;
                }
                let mut sig = self.module.make_signature();
                sig.call_conv = default_conv;
                for param in params {
                    sig.params.push(cranelift::prelude::AbiParam::new(
                        super::CraneliftBackend::map_base_type(&param.type_node),
                    ));
                }
                if *return_type != BaseType::Void {
                    sig.returns.push(cranelift::prelude::AbiParam::new(
                        super::CraneliftBackend::map_base_type(return_type),
                    ));
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
            if let Stmt::Declaration(Decl::FnDecl { name, params, return_type, .. }) = stmt {
                if let Some(&func_id) = funcs_clone.get(name) {
                    self.ctx.clear();
                    self.builder_context = FunctionBuilderContext::new();
                    self.ctx.func.signature.call_conv = default_conv;

                    for param in params {
                        self.ctx.func.signature.params.push(
                            cranelift::prelude::AbiParam::new(
                                super::CraneliftBackend::map_base_type(&param.type_node),
                            ),
                        );
                    }
                    if *return_type != BaseType::Void {
                        self.ctx.func.signature.returns.push(
                            cranelift::prelude::AbiParam::new(
                                super::CraneliftBackend::map_base_type(return_type),
                            ),
                        );
                    }

                    let mut builder = cranelift_frontend::FunctionBuilder::new(
                        &mut self.ctx.func,
                        &mut self.builder_context,
                    );

                    let entry_block = builder.create_block();
                    builder.append_block_params_for_function_params(entry_block);
                    builder.switch_to_block(entry_block);
                    builder.seal_block(entry_block);

                    if *return_type == BaseType::Void {
                        builder.ins().return_(&[]);
                    } else {
                        let zero = builder.ins().iconst(
                            super::CraneliftBackend::map_base_type(return_type),
                            0,
                        );
                        builder.ins().return_(&[zero]);
                    }
                    let target_config = self.module.target_config();
                    builder.finalize(target_config);

                    let _ = self.module.define_function(func_id, &mut self.ctx);
                }
            }
        }
    }

    pub fn finalize(self, out_path: &str) {
        let product = self.module.finish();
        let bytes = product.emit().unwrap();
        std::fs::write(out_path, bytes).unwrap();
        println!("CRANELIFT: Emitted native object to {}", out_path);
    }
}

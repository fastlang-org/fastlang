use crate::frontend::parser::ast::*;
use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Module;

pub mod aot;

pub struct CraneliftBackend {
    module: JITModule,
    builder_context: FunctionBuilderContext,
    ctx: codegen::Context,
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
        let isa = isa_builder.finish(settings::Flags::new(flag_builder)).unwrap();

        let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        let module = JITModule::new(builder);
        let ctx = module.make_context();

        Self {
            module,
            builder_context: FunctionBuilderContext::new(),
            ctx,
        }
    }

    /// Compile program AST directly into Cranelift without intermediate IR
    pub fn compile_ast(&mut self, ast: &[Stmt]) {
        for stmt in ast {
            if let Stmt::Declaration(Decl::FnDecl { name, params, return_type, body, .. }) = stmt {
                self.compile_function(name, params, return_type, body);
            }
        }
    }

    fn compile_function(
        &mut self,
        name: &str,
        params: &[Param],
        return_type: &BaseType,
        _body: &[Stmt],
    ) {
        self.ctx.clear();
        println!("CRANELIFT: Compiling AST function '{}' directly...", name);

        for param in params {
            self.ctx.func.signature.params.push(
                cranelift::prelude::AbiParam::new(Self::map_base_type(&param.type_node))
            );
        }

        if *return_type != BaseType::Void {
            self.ctx.func.signature.returns.push(
                cranelift::prelude::AbiParam::new(Self::map_base_type(return_type))
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

        // Minimal direct lowering return
        if *return_type == BaseType::Void {
            builder.ins().return_(&[]);
        } else {
            let zero = builder.ins().iconst(Self::map_base_type(return_type), 0);
            builder.ins().return_(&[zero]);
        }
        let target_config = self.module.target_config();
        builder.finalize(target_config);
    }

    pub fn finalize(self) {
        println!("CRANELIFT: Direct AST Module Finalized.");
    }

    pub(crate) fn map_base_type(ty: &BaseType) -> Type {
        match ty {
            BaseType::Int(Size::S8) | BaseType::UInt(Size::S8) | BaseType::Bool => types::I8,
            BaseType::Int(Size::S16) | BaseType::UInt(Size::S16) => types::I16,
            BaseType::Int(Size::S32) | BaseType::UInt(Size::S32) | BaseType::Char => types::I32,
            BaseType::Int(Size::S64) | BaseType::UInt(Size::S64) | BaseType::USize | BaseType::ISize => types::I64,
            BaseType::Float(Size::S32) => types::F32,
            BaseType::Float(Size::S64) => types::F64,
            _ => types::I64,
        }
    }
}

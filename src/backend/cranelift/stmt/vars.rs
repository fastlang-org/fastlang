use crate::backend::cranelift::context::FunctionContext;
use crate::backend::cranelift::expr::lower_expr;
use crate::backend::cranelift::types::map_base_type;
use crate::frontend::parser::ast::Decl;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;

pub fn lower_var_decl<M: Module>(
    module: &mut M,
    builder: &mut FunctionBuilder,
    ctx: &mut FunctionContext,
    decl: &Decl,
) {
    if let Decl::VarDecl {
        name,
        type_node,
        value,
        ..
    } = decl
    {
        let ty = map_base_type(type_node);
        let var = ctx.declare_variable(builder, name, ty);
        if let Some(val) = lower_expr(module, builder, ctx, value) {
            builder.def_var(var, val);
        }
    }
}

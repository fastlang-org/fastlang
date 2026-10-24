use crate::backend::cranelift::context::FunctionContext;
use crate::backend::cranelift::expr::lower_expr;
use crate::backend::cranelift::types::coerce_value;
use crate::frontend::parser::ast::Expr;
use cranelift::prelude::*;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;

pub fn lower_return<M: Module>(
    module: &mut M,
    builder: &mut FunctionBuilder,
    ctx: &mut FunctionContext,
    opt_expr: Option<&Expr>,
) {
    if let Some(expr) = opt_expr {
        if let Some(val) = lower_expr(module, builder, ctx, expr) {
            let final_val = if let Some(target_ty) = ctx.current_return_type {
                coerce_value(builder, val, target_ty)
            } else {
                val
            };
            builder.ins().return_(&[final_val]);
            return;
        }
    }
    builder.ins().return_(&[]);
}

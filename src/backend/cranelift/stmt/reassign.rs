use crate::backend::cranelift::context::FunctionContext;
use crate::backend::cranelift::expr::lower_expr;
use crate::frontend::parser::ast::Expr;
use cranelift::prelude::*;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;

pub fn lower_reassign<M: Module>(
    module: &mut M,
    builder: &mut FunctionBuilder,
    ctx: &mut FunctionContext,
    target: &Expr,
    op: &str,
    value: &Expr,
) {
    if let Expr::Identifier(name) = target {
        if let Some((var, _ty)) = ctx.get_variable(name) {
            if let Some(rhs) = lower_expr(module, builder, ctx, value) {
                let final_val = match op {
                    "=" => rhs,
                    "+=" => {
                        let cur = builder.use_var(var);
                        builder.ins().iadd(cur, rhs)
                    }
                    "-=" => {
                        let cur = builder.use_var(var);
                        builder.ins().isub(cur, rhs)
                    }
                    "*=" => {
                        let cur = builder.use_var(var);
                        builder.ins().imul(cur, rhs)
                    }
                    "/=" => {
                        let cur = builder.use_var(var);
                        builder.ins().sdiv(cur, rhs)
                    }
                    "%=" => {
                        let cur = builder.use_var(var);
                        builder.ins().srem(cur, rhs)
                    }
                    _ => rhs,
                };
                builder.def_var(var, final_val);
            }
        }
    }
}

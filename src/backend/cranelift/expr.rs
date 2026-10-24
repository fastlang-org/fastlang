use crate::backend::cranelift::context::FunctionContext;
use crate::backend::cranelift::syscalls;
use crate::frontend::parser::ast::Expr;
use cranelift::prelude::types as cl_types;
use cranelift::prelude::*;
use cranelift_module::{DataDescription, Module};

pub fn lower_expr<M: Module>(
    module: &mut M,
    builder: &mut cranelift_frontend::FunctionBuilder,
    ctx: &mut FunctionContext,
    expr: &Expr,
) -> Option<Value> {
    match expr {
        Expr::LiteralInt(val) => Some(builder.ins().iconst(cl_types::I64, *val as i64)),
        Expr::LiteralUInt(val) => Some(builder.ins().iconst(cl_types::I64, *val as i64)),
        Expr::LiteralFloat(val) => Some(builder.ins().f64const(*val)),
        Expr::LiteralBool(val) => Some(builder.ins().iconst(cl_types::I8, if *val { 1 } else { 0 })),
        Expr::LiteralChar(ch) => Some(builder.ins().iconst(cl_types::I32, *ch as u32 as i64)),
        Expr::LiteralString(s) => {
            let mut desc = DataDescription::new();
            let mut bytes = s.as_bytes().to_vec();
            bytes.push(0);
            desc.define(bytes.into_boxed_slice());
            let data_id = module.declare_anonymous_data(false, false).ok()?;
            module.define_data(data_id, &desc).ok()?;
            let local_id = module.declare_data_in_func(data_id, builder.func);
            let ptr_type = module.target_config().pointer_type();
            Some(builder.ins().symbol_value(ptr_type, local_id))
        }
        Expr::Identifier(name) => {
            if let Some((var, _ty)) = ctx.get_variable(name) {
                Some(builder.use_var(var))
            } else {
                None
            }
        }
        Expr::UnaryOp { operator, operand } => {
            let val = lower_expr(module, builder, ctx, operand)?;
            match operator.as_str() {
                "-" => Some(builder.ins().ineg(val)),
                "!" => Some(builder.ins().bxor_imm_u(val, 1)),
                _ => Some(val),
            }
        }
        Expr::BinaryOp {
            left,
            operator,
            right,
        } => {
            let l_val = lower_expr(module, builder, ctx, left)?;
            let r_val = lower_expr(module, builder, ctx, right)?;
            match operator.as_str() {
                "+" => Some(builder.ins().iadd(l_val, r_val)),
                "-" => Some(builder.ins().isub(l_val, r_val)),
                "*" => Some(builder.ins().imul(l_val, r_val)),
                "/" => Some(builder.ins().sdiv(l_val, r_val)),
                "%" => Some(builder.ins().srem(l_val, r_val)),
                "==" => Some(builder.ins().icmp(IntCC::Equal, l_val, r_val)),
                "!=" => Some(builder.ins().icmp(IntCC::NotEqual, l_val, r_val)),
                "<" => Some(builder.ins().icmp(IntCC::SignedLessThan, l_val, r_val)),
                "<=" => Some(builder.ins().icmp(IntCC::SignedLessThanOrEqual, l_val, r_val)),
                ">" => Some(builder.ins().icmp(IntCC::SignedGreaterThan, l_val, r_val)),
                ">=" => Some(builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, l_val, r_val)),
                "&&" | "&" => Some(builder.ins().band(l_val, r_val)),
                "||" | "|" => Some(builder.ins().bor(l_val, r_val)),
                "^" => Some(builder.ins().bxor(l_val, r_val)),
                "<<" => Some(builder.ins().ishl(l_val, r_val)),
                ">>" => Some(builder.ins().sshr(l_val, r_val)),
                _ => Some(l_val),
            }
        }
        Expr::Call { callee, args, .. } | Expr::MacroCall { callee, args } => {
            let mut arg_vals = Vec::new();
            for arg in args {
                if let Some(val) = lower_expr(module, builder, ctx, arg) {
                    arg_vals.push(val);
                }
            }

            if let Expr::NamespaceAccess {
                namespace,
                property,
            } = &**callee
            {
                if namespace == "@sys" {
                    if let Expr::Identifier(name) = &**property {
                        return syscalls::lower_syscall(module, builder, name, &arg_vals);
                    }
                }
            }

            if let Expr::Identifier(fn_name) = &**callee {
                if let Some(&func_id) = ctx.func_ids.get(fn_name) {
                    let local_func = module.declare_func_in_func(func_id, builder.func);
                    let sig_ref = builder.func.dfg.ext_funcs[local_func].signature;
                    let sig = builder.func.dfg.signatures[sig_ref].clone();
                    let mut coerced_args = Vec::new();
                    for (i, &arg_val) in arg_vals.iter().enumerate() {
                        if i < sig.params.len() {
                            let target_ty = sig.params[i].value_type;
                            coerced_args.push(crate::backend::cranelift::types::coerce_value(
                                builder, arg_val, target_ty,
                            ));
                        } else {
                            coerced_args.push(arg_val);
                        }
                    }
                    let call = builder.ins().call(local_func, &coerced_args);
                    let results = builder.inst_results(call);
                    return results.first().copied();
                }
            }
            None
        }
        Expr::Cast { expr, .. } => lower_expr(module, builder, ctx, expr),
        _ => None,
    }
}

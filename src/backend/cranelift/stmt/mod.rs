pub mod control_flow;
pub mod reassign;
pub mod returns;
pub mod vars;

use crate::backend::cranelift::context::{is_current_block_terminated, FunctionContext};
use crate::backend::cranelift::expr::lower_expr;
use crate::frontend::parser::ast::{Decl, Stmt};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;

pub fn lower_stmt<M: Module>(
    module: &mut M,
    builder: &mut FunctionBuilder,
    ctx: &mut FunctionContext,
    stmt: &Stmt,
) {
    if is_current_block_terminated(builder) {
        return;
    }

    match stmt {
        Stmt::Declaration(decl) => {
            if let Decl::VarDecl { .. } = decl {
                vars::lower_var_decl(module, builder, ctx, decl);
            }
        }
        Stmt::ReassignStmt { target, op, value } => {
            reassign::lower_reassign(module, builder, ctx, target, op, value);
        }
        Stmt::IfStmt {
            condition,
            then_block,
            else_block,
        } => {
            control_flow::lower_if(
                module,
                builder,
                ctx,
                condition,
                then_block,
                else_block.as_deref(),
            );
        }
        Stmt::WhileStmt { condition, body } => {
            control_flow::lower_while(module, builder, ctx, condition, body);
        }
        Stmt::BreakStmt => control_flow::lower_break(builder, ctx),
        Stmt::ContinueStmt => control_flow::lower_continue(builder, ctx),
        Stmt::ReturnStmt(opt_expr) => returns::lower_return(module, builder, ctx, opt_expr.as_ref()),
        Stmt::ExpressionStmt(expr) | Stmt::CallStmt(expr) => {
            let _ = lower_expr(module, builder, ctx, expr);
        }
        Stmt::Block(stmts) | Stmt::ThisBlock(stmts) => {
            for s in stmts {
                lower_stmt(module, builder, ctx, s);
            }
        }
        _ => {}
    }
}

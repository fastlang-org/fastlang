use crate::backend::cranelift::context::{is_current_block_terminated, FunctionContext};
use crate::backend::cranelift::expr::lower_expr;
use crate::backend::cranelift::stmt::lower_stmt;
use crate::frontend::parser::ast::{EitherBlock, Expr, Stmt};
use cranelift::prelude::*;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;

pub fn lower_if<M: Module>(
    module: &mut M,
    builder: &mut FunctionBuilder,
    ctx: &mut FunctionContext,
    condition: &Expr,
    then_block_stmts: &[Stmt],
    else_block_stmts: Option<&[Stmt]>,
) {
    let cond_val = match lower_expr(module, builder, ctx, condition) {
        Some(v) => v,
        None => return,
    };

    let then_block = builder.create_block();
    let else_block = builder.create_block();
    let merge_block = builder.create_block();

    builder
        .ins()
        .brif(cond_val, then_block, &[], else_block, &[]);

    // Then block
    builder.switch_to_block(then_block);
    builder.seal_block(then_block);
    for stmt in then_block_stmts {
        lower_stmt(module, builder, ctx, stmt);
    }
    if !is_current_block_terminated(builder) {
        builder.ins().jump(merge_block, &[]);
    }

    // Else block
    builder.switch_to_block(else_block);
    builder.seal_block(else_block);
    if let Some(stmts) = else_block_stmts {
        for stmt in stmts {
            lower_stmt(module, builder, ctx, stmt);
        }
    }
    if !is_current_block_terminated(builder) {
        builder.ins().jump(merge_block, &[]);
    }

    // Merge block
    builder.switch_to_block(merge_block);
    builder.seal_block(merge_block);
}

pub fn lower_while<M: Module>(
    module: &mut M,
    builder: &mut FunctionBuilder,
    ctx: &mut FunctionContext,
    condition: &Expr,
    body: &EitherBlock,
) {
    let header_block = builder.create_block();
    let body_block = builder.create_block();
    let exit_block = builder.create_block();

    builder.ins().jump(header_block, &[]);

    // Header: evaluate condition
    builder.switch_to_block(header_block);
    if let Some(cond_val) = lower_expr(module, builder, ctx, condition) {
        builder
            .ins()
            .brif(cond_val, body_block, &[], exit_block, &[]);
    } else {
        builder.ins().jump(exit_block, &[]);
    }

    // Body
    builder.switch_to_block(body_block);
    builder.seal_block(body_block);
    ctx.push_loop(header_block, exit_block);

    match body {
        EitherBlock::Inline(stmts) => {
            for stmt in stmts {
                lower_stmt(module, builder, ctx, stmt);
            }
        }
        EitherBlock::External(expr) => {
            let _ = lower_expr(module, builder, ctx, expr);
        }
    }

    ctx.pop_loop();

    if !is_current_block_terminated(builder) {
        builder.ins().jump(header_block, &[]);
    }

    builder.seal_block(header_block);

    // Exit
    builder.switch_to_block(exit_block);
    builder.seal_block(exit_block);
}

pub fn lower_break(builder: &mut FunctionBuilder, ctx: &FunctionContext) {
    if let Some(loop_scope) = ctx.current_loop() {
        builder.ins().jump(loop_scope.break_block, &[]);
    }
}

pub fn lower_continue(builder: &mut FunctionBuilder, ctx: &FunctionContext) {
    if let Some(loop_scope) = ctx.current_loop() {
        builder.ins().jump(loop_scope.continue_block, &[]);
    }
}

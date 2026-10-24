pub mod io;
pub mod memory;

use cranelift::prelude::*;
use cranelift_module::Module;

pub fn lower_syscall<M: Module>(
    module: &mut M,
    builder: &mut cranelift_frontend::FunctionBuilder,
    name: &str,
    args: &[Value],
) -> Option<Value> {
    match name {
        "write" | "read" | "open" | "close" => io::lower_io_syscall(module, builder, name, args),
        "alloc" | "malloc" | "free" | "exit" => memory::lower_mem_syscall(module, builder, name, args),
        _ => None,
    }
}

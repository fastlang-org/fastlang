use cranelift::prelude::types as cl_types;
use cranelift::prelude::*;
use cranelift_module::{Linkage, Module};

pub fn lower_mem_syscall<M: Module>(
    module: &mut M,
    builder: &mut cranelift_frontend::FunctionBuilder,
    name: &str,
    args: &[Value],
) -> Option<Value> {
    let ptr_type = module.target_config().pointer_type();
    match name {
        "alloc" | "malloc" => {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(ptr_type));
            sig.returns.push(AbiParam::new(ptr_type));
            let func_id = module.declare_function("malloc", Linkage::Import, &sig).ok()?;
            let local_func = module.declare_func_in_func(func_id, builder.func);
            let call = builder.ins().call(local_func, args);
            Some(builder.inst_results(call)[0])
        }
        "free" => {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(ptr_type));
            let func_id = module.declare_function("free", Linkage::Import, &sig).ok()?;
            let local_func = module.declare_func_in_func(func_id, builder.func);
            builder.ins().call(local_func, args);
            None
        }
        "exit" => {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(cl_types::I32));
            let func_id = module.declare_function("exit", Linkage::Import, &sig).ok()?;
            let local_func = module.declare_func_in_func(func_id, builder.func);
            builder.ins().call(local_func, args);
            None
        }
        _ => None,
    }
}

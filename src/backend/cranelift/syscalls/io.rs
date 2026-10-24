use crate::backend::cranelift::types::coerce_value;
use cranelift::prelude::types as cl_types;
use cranelift::prelude::*;
use cranelift_module::{Linkage, Module};

pub fn lower_io_syscall<M: Module>(
    module: &mut M,
    builder: &mut cranelift_frontend::FunctionBuilder,
    name: &str,
    args: &[Value],
) -> Option<Value> {
    let ptr_type = module.target_config().pointer_type();
    match name {
        "write" => {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(cl_types::I32));
            sig.params.push(AbiParam::new(ptr_type));
            sig.params.push(AbiParam::new(ptr_type));
            sig.returns.push(AbiParam::new(ptr_type));
            let func_id = module.declare_function("write", Linkage::Import, &sig).ok()?;
            let local_func = module.declare_func_in_func(func_id, builder.func);
            let a0 = coerce_value(builder, args[0], cl_types::I32);
            let a1 = coerce_value(builder, args[1], ptr_type);
            let a2 = coerce_value(builder, args[2], ptr_type);
            let call = builder.ins().call(local_func, &[a0, a1, a2]);
            Some(builder.inst_results(call)[0])
        }
        "read" => {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(cl_types::I32));
            sig.params.push(AbiParam::new(ptr_type));
            sig.params.push(AbiParam::new(ptr_type));
            sig.returns.push(AbiParam::new(ptr_type));
            let func_id = module.declare_function("read", Linkage::Import, &sig).ok()?;
            let local_func = module.declare_func_in_func(func_id, builder.func);
            let a0 = coerce_value(builder, args[0], cl_types::I32);
            let a1 = coerce_value(builder, args[1], ptr_type);
            let a2 = coerce_value(builder, args[2], ptr_type);
            let call = builder.ins().call(local_func, &[a0, a1, a2]);
            Some(builder.inst_results(call)[0])
        }
        "open" => {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(ptr_type));
            sig.params.push(AbiParam::new(cl_types::I32));
            sig.params.push(AbiParam::new(cl_types::I32));
            sig.returns.push(AbiParam::new(cl_types::I32));
            let func_id = module.declare_function("open", Linkage::Import, &sig).ok()?;
            let local_func = module.declare_func_in_func(func_id, builder.func);
            let a0 = coerce_value(builder, args[0], ptr_type);
            let a1 = coerce_value(builder, args[1], cl_types::I32);
            let a2 = coerce_value(builder, args[2], cl_types::I32);
            let call = builder.ins().call(local_func, &[a0, a1, a2]);
            Some(builder.inst_results(call)[0])
        }
        "close" => {
            let mut sig = module.make_signature();
            sig.params.push(AbiParam::new(cl_types::I32));
            sig.returns.push(AbiParam::new(cl_types::I32));
            let func_id = module.declare_function("close", Linkage::Import, &sig).ok()?;
            let local_func = module.declare_func_in_func(func_id, builder.func);
            let a0 = coerce_value(builder, args[0], cl_types::I32);
            let call = builder.ins().call(local_func, &[a0]);
            Some(builder.inst_results(call)[0])
        }
        _ => None,
    }
}

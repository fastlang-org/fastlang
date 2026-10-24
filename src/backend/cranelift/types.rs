use crate::frontend::parser::ast::{BaseType, Size};
use cranelift::prelude::types as cl_types;
use cranelift::prelude::{InstBuilder, Type};

pub fn map_base_type(ty: &BaseType) -> Type {
    match ty {
        BaseType::Int(Size::S8) | BaseType::UInt(Size::S8) | BaseType::Bool => cl_types::I8,
        BaseType::Int(Size::S16) | BaseType::UInt(Size::S16) => cl_types::I16,
        BaseType::Int(Size::S32) | BaseType::UInt(Size::S32) | BaseType::Char => cl_types::I32,
        BaseType::Int(Size::S64)
        | BaseType::UInt(Size::S64)
        | BaseType::USize
        | BaseType::ISize
        | BaseType::RawPointer(_)
        | BaseType::Str => cl_types::I64,
        BaseType::Float(Size::S32) => cl_types::F32,
        BaseType::Float(Size::S64) => cl_types::F64,
        _ => cl_types::I64,
    }
}

pub fn coerce_value(
    builder: &mut cranelift_frontend::FunctionBuilder,
    val: cranelift::prelude::Value,
    target_ty: Type,
) -> cranelift::prelude::Value {
    let src_ty = builder.func.dfg.value_type(val);
    if src_ty == target_ty {
        return val;
    }
    if src_ty.is_int() && target_ty.is_int() {
        if src_ty.bits() > target_ty.bits() {
            builder.ins().ireduce(target_ty, val)
        } else {
            builder.ins().uextend(target_ty, val)
        }
    } else {
        val
    }
}

use cranelift::prelude::*;
use cranelift_frontend::Variable;
use cranelift_module::FuncId;
use std::collections::HashMap;

pub struct LoopScope {
    pub continue_block: Block,
    pub break_block: Block,
}

pub struct FunctionContext {
    pub variables: HashMap<String, (Variable, Type)>,
    pub loop_stack: Vec<LoopScope>,
    pub func_ids: HashMap<String, FuncId>,
    pub current_return_type: Option<Type>,
}

impl FunctionContext {
    pub fn new(func_ids: HashMap<String, FuncId>, current_return_type: Option<Type>) -> Self {
        Self {
            variables: HashMap::new(),
            loop_stack: Vec::new(),
            func_ids,
            current_return_type,
        }
    }

    pub fn declare_variable(
        &mut self,
        builder: &mut cranelift_frontend::FunctionBuilder,
        name: &str,
        ty: Type,
    ) -> Variable {
        let var = builder.declare_var(ty);
        self.variables.insert(name.to_string(), (var, ty));
        var
    }

    pub fn get_variable(&self, name: &str) -> Option<(Variable, Type)> {
        self.variables.get(name).copied()
    }

    pub fn push_loop(&mut self, continue_block: Block, break_block: Block) {
        self.loop_stack.push(LoopScope {
            continue_block,
            break_block,
        });
    }

    pub fn pop_loop(&mut self) -> Option<LoopScope> {
        self.loop_stack.pop()
    }

    pub fn current_loop(&self) -> Option<&LoopScope> {
        self.loop_stack.last()
    }
}

pub fn is_current_block_terminated(builder: &cranelift_frontend::FunctionBuilder) -> bool {
    if let Some(block) = builder.current_block() {
        if let Some(last_inst) = builder.func.layout.last_inst(block) {
            return builder.func.dfg.insts[last_inst].opcode().is_terminator();
        }
    }
    false
}

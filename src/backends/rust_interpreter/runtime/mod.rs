//! Runtime execution scaffolding.
//!
//! WHAT: defines the core runtime containers used by the interpreter engine.
//! WHY: the runtime state should stay separate from lowering so CTFE can reuse the same engine later.

use crate::backends::rust_interpreter::error::InterpreterBackendError;
use crate::backends::rust_interpreter::exec_ir::{
    ExecBlock, ExecBlockId, ExecConstId, ExecConstValue, ExecFunction, ExecFunctionId,
    ExecInstruction, ExecProgram, ExecTerminator,
};
use crate::backends::rust_interpreter::heap::{Heap, HeapObject, StringObject};
pub(crate) use crate::backends::rust_interpreter::request::InterpreterExecutionPolicy as ExecutionPolicy;
use crate::backends::rust_interpreter::value::Value;

#[derive(Debug, Clone)]
pub(crate) struct RuntimeEngine {
    pub program: ExecProgram,
    pub heap: Heap,
    pub policy: ExecutionPolicy,
    pub stack: FrameStack,
}

impl RuntimeEngine {
    pub(crate) fn new(program: ExecProgram, policy: ExecutionPolicy) -> Self {
        Self {
            program,
            heap: Heap::new(),
            policy,
            stack: FrameStack::default(),
        }
    }

    pub(crate) fn execute_start(&mut self) -> Result<Value, InterpreterBackendError> {
        let Some(entry_function) = self.program.module.entry_function else {
            return Err(InterpreterBackendError::Execution {
                message: "Rust interpreter runtime has no entry function to execute".to_owned(),
            });
        };

        self.execute_function(entry_function)
    }

    fn execute_function(
        &mut self,
        function_id: ExecFunctionId,
    ) -> Result<Value, InterpreterBackendError> {
        let function = self.function_by_id(function_id)?.clone();

        if !function.parameter_slots.is_empty() {
            return Err(InterpreterBackendError::Execution {
                message: format!(
                    "Rust interpreter runtime cannot execute function '{}' with parameters yet",
                    function.debug_name
                ),
            });
        }

        self.stack.frames.push(CallFrame {
            function_id,
            block_id: function.entry_block,
            locals: LocalStorage::with_slot_count(function.locals.len()),
        });

        loop {
            let current_block_id = self.current_frame()?.block_id;
            let block = block_by_id(&function, current_block_id)?.clone();

            for instruction in &block.instructions {
                self.execute_instruction(instruction)?;
            }

            match &block.terminator {
                ExecTerminator::Return { value } => {
                    let result = match value {
                        Some(local_id) => self.read_local(*local_id)?,
                        None => Value::Unit,
                    };

                    self.stack.frames.pop();
                    return Ok(result);
                }

                ExecTerminator::Jump { target } => {
                    self.current_frame_mut()?.block_id = *target;
                }

                ExecTerminator::BranchBool {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let condition_value = self.read_local(*condition)?;
                    let branch_target = match condition_value {
                        Value::Bool(true) => *then_block,
                        Value::Bool(false) => *else_block,
                        other => {
                            return Err(InterpreterBackendError::Execution {
                                message: format!(
                                    "Rust interpreter runtime expected bool branch condition, found {other:?}"
                                ),
                            });
                        }
                    };

                    self.current_frame_mut()?.block_id = branch_target;
                }

                ExecTerminator::PendingLowering { description } => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Rust interpreter runtime reached pending-lowering terminator: {description}"
                        ),
                    });
                }

                ExecTerminator::UnreachableTrap => {
                    return Err(InterpreterBackendError::Execution {
                        message: "Rust interpreter runtime hit unreachable trap".to_owned(),
                    });
                }
            }
        }
    }

    fn execute_instruction(
        &mut self,
        instruction: &ExecInstruction,
    ) -> Result<(), InterpreterBackendError> {
        match instruction {
            ExecInstruction::LoadConst { target, const_id } => {
                let value = self.materialize_const(*const_id)?;
                self.write_local(*target, value)?;
            }

            ExecInstruction::ReadLocal { target, source } => {
                let value = self.read_local(*source)?;
                self.write_local(*target, value)?;
            }

            ExecInstruction::CopyLocal { target, source } => {
                let value = self.copy_local_value(*source)?;
                self.write_local(*target, value)?;
            }
        }

        Ok(())
    }

    fn materialize_const(
        &mut self,
        const_id: ExecConstId,
    ) -> Result<Value, InterpreterBackendError> {
        let const_value = self.const_value_by_id(const_id)?.clone();

        match const_value {
            ExecConstValue::Unit => Ok(Value::Unit),
            ExecConstValue::Bool(value) => Ok(Value::Bool(value)),
            ExecConstValue::Int(value) => Ok(Value::Int(value)),
            ExecConstValue::Float(value) => Ok(Value::Float(value)),
            ExecConstValue::Char(value) => Ok(Value::Char(value)),
            ExecConstValue::String(text) => {
                let handle = self
                    .heap
                    .allocate(HeapObject::String(StringObject { text }));
                Ok(Value::Handle(handle))
            }
        }
    }

    fn copy_local_value(
        &mut self,
        local_id: crate::backends::rust_interpreter::exec_ir::ExecLocalId,
    ) -> Result<Value, InterpreterBackendError> {
        let value = self.read_local(local_id)?;

        match value {
            Value::Unit => Ok(Value::Unit),
            Value::Bool(value) => Ok(Value::Bool(value)),
            Value::Int(value) => Ok(Value::Int(value)),
            Value::Float(value) => Ok(Value::Float(value)),
            Value::Char(value) => Ok(Value::Char(value)),
            Value::Handle(_) => Err(InterpreterBackendError::Execution {
                message: "Rust interpreter runtime does not support explicit copy of heap-backed values yet"
                    .to_owned(),
            }),
        }
    }

    fn function_by_id(
        &self,
        function_id: ExecFunctionId,
    ) -> Result<&ExecFunction, InterpreterBackendError> {
        self.program
            .module
            .functions
            .iter()
            .find(|function| function.id == function_id)
            .ok_or_else(|| InterpreterBackendError::Execution {
                message: format!(
                    "Rust interpreter runtime could not resolve function {:?}",
                    function_id
                ),
            })
    }

    fn const_value_by_id(
        &self,
        const_id: ExecConstId,
    ) -> Result<&ExecConstValue, InterpreterBackendError> {
        self.program
            .module
            .constants
            .iter()
            .find(|constant| constant.id == const_id)
            .map(|constant| &constant.value)
            .ok_or_else(|| InterpreterBackendError::Execution {
                message: format!(
                    "Rust interpreter runtime could not resolve constant {:?}",
                    const_id
                ),
            })
    }

    fn current_frame(&self) -> Result<&CallFrame, InterpreterBackendError> {
        self.stack
            .frames
            .last()
            .ok_or_else(|| InterpreterBackendError::Execution {
                message: "Rust interpreter runtime has no active call frame".to_owned(),
            })
    }

    fn current_frame_mut(&mut self) -> Result<&mut CallFrame, InterpreterBackendError> {
        self.stack
            .frames
            .last_mut()
            .ok_or_else(|| InterpreterBackendError::Execution {
                message: "Rust interpreter runtime has no active call frame".to_owned(),
            })
    }

    fn read_local(
        &self,
        local_id: crate::backends::rust_interpreter::exec_ir::ExecLocalId,
    ) -> Result<Value, InterpreterBackendError> {
        let frame = self.current_frame()?;
        frame
            .locals
            .slots
            .get(local_id.0 as usize)
            .cloned()
            .ok_or_else(|| InterpreterBackendError::Execution {
                message: format!(
                    "Rust interpreter runtime local slot {:?} is out of bounds",
                    local_id
                ),
            })
    }

    fn write_local(
        &mut self,
        local_id: crate::backends::rust_interpreter::exec_ir::ExecLocalId,
        value: Value,
    ) -> Result<(), InterpreterBackendError> {
        let frame = self.current_frame_mut()?;
        let Some(slot) = frame.locals.slots.get_mut(local_id.0 as usize) else {
            return Err(InterpreterBackendError::Execution {
                message: format!(
                    "Rust interpreter runtime local slot {:?} is out of bounds",
                    local_id
                ),
            });
        };

        *slot = value;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CallFrame {
    pub function_id: ExecFunctionId,
    pub block_id: ExecBlockId,
    pub locals: LocalStorage,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FrameStack {
    pub frames: Vec<CallFrame>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct LocalStorage {
    pub slots: Vec<Value>,
}

impl LocalStorage {
    pub(crate) fn with_slot_count(slot_count: usize) -> Self {
        Self {
            slots: vec![Value::Unit; slot_count],
        }
    }
}

fn block_by_id(
    function: &ExecFunction,
    block_id: ExecBlockId,
) -> Result<&ExecBlock, InterpreterBackendError> {
    function
        .blocks
        .iter()
        .find(|block| block.id == block_id)
        .ok_or_else(|| InterpreterBackendError::Execution {
            message: format!(
                "Rust interpreter runtime could not resolve block {:?} in function '{}'",
                block_id, function.debug_name
            ),
        })
}

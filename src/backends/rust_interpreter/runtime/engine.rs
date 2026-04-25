//! Runtime execution engine.
//!
//! WHAT: defines the core runtime containers used by the interpreter engine.
//! WHY: the runtime state should stay separate from lowering so CTFE can reuse the same engine later.

use crate::backends::rust_interpreter::error::InterpreterBackendError;
use crate::backends::rust_interpreter::exec_ir::{
    ExecBlockId, ExecConstId, ExecConstValue, ExecFunctionId, ExecInstruction, ExecLocalId,
    ExecProgram, ExecTerminator,
};
use crate::backends::rust_interpreter::heap::{Heap, HeapObject, StringObject};
pub(crate) use crate::backends::rust_interpreter::request::InterpreterExecutionPolicy as ExecutionPolicy;
use crate::backends::rust_interpreter::runtime::lookups::{
    build_block_index, build_const_index, build_function_index,
};
use crate::backends::rust_interpreter::value::Value;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub(crate) struct RuntimeEngine {
    pub program: ExecProgram,
    pub heap: Heap,
    pub policy: ExecutionPolicy,
    pub stack: FrameStack,
    pub(crate) function_index_by_id: FxHashMap<ExecFunctionId, usize>,
    pub(crate) block_index_by_function: FxHashMap<ExecFunctionId, FxHashMap<ExecBlockId, usize>>,
    pub(crate) const_index_by_id: FxHashMap<ExecConstId, usize>,
}

impl RuntimeEngine {
    pub(crate) fn new(program: ExecProgram, policy: ExecutionPolicy) -> Self {
        let function_index_by_id = build_function_index(&program);
        let block_index_by_function = build_block_index(&program);
        let const_index_by_id = build_const_index(&program);

        Self {
            program,
            heap: Heap::new(),
            policy,
            stack: FrameStack::default(),
            function_index_by_id,
            block_index_by_function,
            const_index_by_id,
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
        let (entry_block, local_count, has_parameters, function_debug_name) = {
            let function = self.function_by_id(function_id)?;
            (
                function.entry_block,
                function.locals.len(),
                !function.parameter_slots.is_empty(),
                function.debug_name.clone(),
            )
        };

        if has_parameters {
            return Err(InterpreterBackendError::Execution {
                message: format!(
                    "Rust interpreter runtime cannot execute function '{function_debug_name}' with parameters yet"
                ),
            });
        }

        self.stack.frames.push(CallFrame {
            function_id,
            block_id: entry_block,
            locals: LocalStorage::with_slot_count(local_count),
        });

        loop {
            let (current_function_id, current_block_id) = {
                let frame = self.current_frame()?;
                (frame.function_id, frame.block_id)
            };

            let (instructions, terminator) = {
                let block = self
                    .block_by_ids(current_function_id, current_block_id)?
                    .clone();
                (block.instructions.clone(), block.terminator.clone())
            };

            for instruction in &instructions {
                self.execute_instruction(instruction)?;
            }

            match terminator {
                ExecTerminator::Return { value } => {
                    let result = match value {
                        Some(local_id) => self.read_local(local_id)?,
                        None => Value::Unit,
                    };

                    self.stack.frames.pop();
                    return Ok(result);
                }

                ExecTerminator::Jump { target } => {
                    self.current_frame_mut()?.block_id = target;
                }

                ExecTerminator::BranchBool {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let condition_value = self.read_local(condition)?;
                    let branch_target = match condition_value {
                        Value::Bool(true) => then_block,
                        Value::Bool(false) => else_block,
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

            ExecInstruction::BinaryOp {
                left,
                operator,
                right,
                destination,
            } => {
                self.execute_binary_op(*left, *operator, *right, *destination)?;
            }

            ExecInstruction::UnaryOp {
                operand,
                operator,
                destination,
            } => {
                self.execute_unary_op(*operand, *operator, *destination)?;
            }
        }

        Ok(())
    }

    fn execute_binary_op(
        &mut self,
        left: ExecLocalId,
        operator: crate::backends::rust_interpreter::exec_ir::ExecBinaryOperator,
        right: ExecLocalId,
        destination: ExecLocalId,
    ) -> Result<(), InterpreterBackendError> {
        use crate::backends::rust_interpreter::exec_ir::ExecBinaryOperator;

        let left_value = self.read_local(left)?;
        let right_value = self.read_local(right)?;

        let result = match operator {
            ExecBinaryOperator::Add => match (left_value, right_value) {
                (Value::Int(l), Value::Int(r)) => Value::Int(l + r),
                (Value::Float(l), Value::Float(r)) => Value::Float(l + r),
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in Add operation: expected Int or Float operands, found {l:?} and {r:?}"
                        ),
                    });
                }
            },

            ExecBinaryOperator::Subtract => match (left_value, right_value) {
                (Value::Int(l), Value::Int(r)) => Value::Int(l - r),
                (Value::Float(l), Value::Float(r)) => Value::Float(l - r),
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in Subtract operation: expected Int or Float operands, found {l:?} and {r:?}"
                        ),
                    });
                }
            },

            ExecBinaryOperator::Multiply => match (left_value, right_value) {
                (Value::Int(l), Value::Int(r)) => Value::Int(l * r),
                (Value::Float(l), Value::Float(r)) => Value::Float(l * r),
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in Multiply operation: expected Int or Float operands, found {l:?} and {r:?}"
                        ),
                    });
                }
            },

            ExecBinaryOperator::Divide => match (left_value, right_value) {
                (Value::Int(l), Value::Int(r)) => Value::Float(l as f64 / r as f64),
                (Value::Int(l), Value::Float(r)) => Value::Float(l as f64 / r),
                (Value::Float(l), Value::Int(r)) => Value::Float(l / r as f64),
                (Value::Float(l), Value::Float(r)) => Value::Float(l / r),
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in Divide operation: expected Int or Float operands, found {l:?} and {r:?}"
                        ),
                    });
                }
            },

            ExecBinaryOperator::IntDivide => match (left_value, right_value) {
                (Value::Int(l), Value::Int(r)) => {
                    if r == 0 {
                        return Err(InterpreterBackendError::Execution {
                            message: "Division by zero".to_owned(),
                        });
                    }
                    Value::Int(l / r)
                }
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in IntDivide operation: expected Int operands, found {l:?} and {r:?}"
                        ),
                    });
                }
            },

            ExecBinaryOperator::Modulo => match (left_value, right_value) {
                (Value::Int(l), Value::Int(r)) => {
                    if r == 0 {
                        return Err(InterpreterBackendError::Execution {
                            message: "Modulo by zero".to_owned(),
                        });
                    }
                    Value::Int(l.rem_euclid(r))
                }
                (Value::Float(l), Value::Float(r)) => {
                    if r == 0.0 {
                        return Err(InterpreterBackendError::Execution {
                            message: "Modulo by zero".to_owned(),
                        });
                    }
                    Value::Float(l.rem_euclid(r))
                }
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in Modulo operation: expected Int or Float operands, found {l:?} and {r:?}"
                        ),
                    });
                }
            },

            ExecBinaryOperator::Equal => match (left_value, right_value) {
                (Value::Int(l), Value::Int(r)) => Value::Bool(l == r),
                (Value::Float(l), Value::Float(r)) => Value::Bool(l == r),
                (Value::Bool(l), Value::Bool(r)) => Value::Bool(l == r),
                (Value::Char(l), Value::Char(r)) => Value::Bool(l == r),
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in Equal operation: operands must have the same type, found {l:?} and {r:?}"
                        ),
                    });
                }
            },

            ExecBinaryOperator::NotEqual => match (left_value, right_value) {
                (Value::Int(l), Value::Int(r)) => Value::Bool(l != r),
                (Value::Float(l), Value::Float(r)) => Value::Bool(l != r),
                (Value::Bool(l), Value::Bool(r)) => Value::Bool(l != r),
                (Value::Char(l), Value::Char(r)) => Value::Bool(l != r),
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in NotEqual operation: operands must have the same type, found {l:?} and {r:?}"
                        ),
                    });
                }
            },

            ExecBinaryOperator::LessThan => match (left_value, right_value) {
                (Value::Int(l), Value::Int(r)) => Value::Bool(l < r),
                (Value::Float(l), Value::Float(r)) => Value::Bool(l < r),
                (Value::Char(l), Value::Char(r)) => Value::Bool(l < r),
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in LessThan operation: expected Int, Float, or Char operands, found {l:?} and {r:?}"
                        ),
                    });
                }
            },

            ExecBinaryOperator::LessThanOrEqual => match (left_value, right_value) {
                (Value::Int(l), Value::Int(r)) => Value::Bool(l <= r),
                (Value::Float(l), Value::Float(r)) => Value::Bool(l <= r),
                (Value::Char(l), Value::Char(r)) => Value::Bool(l <= r),
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in LessThanOrEqual operation: expected Int, Float, or Char operands, found {l:?} and {r:?}"
                        ),
                    });
                }
            },

            ExecBinaryOperator::GreaterThan => match (left_value, right_value) {
                (Value::Int(l), Value::Int(r)) => Value::Bool(l > r),
                (Value::Float(l), Value::Float(r)) => Value::Bool(l > r),
                (Value::Char(l), Value::Char(r)) => Value::Bool(l > r),
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in GreaterThan operation: expected Int, Float, or Char operands, found {l:?} and {r:?}"
                        ),
                    });
                }
            },

            ExecBinaryOperator::GreaterThanOrEqual => match (left_value, right_value) {
                (Value::Int(l), Value::Int(r)) => Value::Bool(l >= r),
                (Value::Float(l), Value::Float(r)) => Value::Bool(l >= r),
                (Value::Char(l), Value::Char(r)) => Value::Bool(l >= r),
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in GreaterThanOrEqual operation: expected Int, Float, or Char operands, found {l:?} and {r:?}"
                        ),
                    });
                }
            },

            ExecBinaryOperator::And => match (left_value, right_value) {
                (Value::Bool(l), Value::Bool(r)) => Value::Bool(l && r),
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in And operation: expected Bool operands, found {l:?} and {r:?}"
                        ),
                    });
                }
            },

            ExecBinaryOperator::Or => match (left_value, right_value) {
                (Value::Bool(l), Value::Bool(r)) => Value::Bool(l || r),
                (l, r) => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in Or operation: expected Bool operands, found {l:?} and {r:?}"
                        ),
                    });
                }
            },
        };

        self.write_local(destination, result)?;
        Ok(())
    }

    fn execute_unary_op(
        &mut self,
        operand: ExecLocalId,
        operator: crate::backends::rust_interpreter::exec_ir::ExecUnaryOperator,
        destination: ExecLocalId,
    ) -> Result<(), InterpreterBackendError> {
        use crate::backends::rust_interpreter::exec_ir::ExecUnaryOperator;

        let operand_value = self.read_local(operand)?;

        let result = match operator {
            ExecUnaryOperator::Negate => match operand_value {
                Value::Int(v) => Value::Int(-v),
                Value::Float(v) => Value::Float(-v),
                other => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in Negate operation: expected Int or Float operand, found {other:?}",
                        ),
                    });
                }
            },

            ExecUnaryOperator::Not => match operand_value {
                Value::Bool(v) => Value::Bool(!v),
                other => {
                    return Err(InterpreterBackendError::Execution {
                        message: format!(
                            "Type mismatch in Not operation: expected Bool operand, found {other:?}",
                        ),
                    });
                }
            },
        };

        self.write_local(destination, result)?;
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
        local_id: ExecLocalId,
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

    fn read_local(&self, local_id: ExecLocalId) -> Result<Value, InterpreterBackendError> {
        let frame = self.current_frame()?;
        frame
            .locals
            .slots
            .get(local_id.0 as usize)
            .cloned()
            .ok_or_else(|| InterpreterBackendError::Execution {
                message: format!(
                    "Rust interpreter runtime local slot {local_id:?} is out of bounds",
                ),
            })
    }

    fn write_local(
        &mut self,
        local_id: ExecLocalId,
        value: Value,
    ) -> Result<(), InterpreterBackendError> {
        let frame = self.current_frame_mut()?;
        let Some(slot) = frame.locals.slots.get_mut(local_id.0 as usize) else {
            return Err(InterpreterBackendError::Execution {
                message: format!(
                    "Rust interpreter runtime local slot {local_id:?} is out of bounds",
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

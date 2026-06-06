use super::ir::{
    BuiltinFn, CompiledExpr, ModifierExpr, ModifierIrProgram, ModifierIrStmt, ModifierOverrides,
    apply_binary_op, eval_abs, eval_atan2, eval_ceil, eval_clamp, eval_cos, eval_exp, eval_floor,
    eval_format, eval_lerp, eval_log, eval_max, eval_min, eval_sin, eval_sqrt, eval_tan,
    make_vec_value,
};
use crate::ast::BinaryOp;
use crate::timeline::{Environment, EvalError, Value};
use std::fmt;

/// Bytecode instruction for the modifier VM.
#[derive(Clone, Debug, PartialEq)]
pub enum Instruction {
    /// Load a constant from the constant pool onto the stack.
    LoadConst(usize),
    /// Load a variable from the environment onto the stack.
    LoadEnv(String),
    /// Pop a value from the stack and store it in the environment.
    StoreEnv(String),
    /// Pop N values from the stack and combine them into a vector.
    MakeVec(usize),
    /// Pop a value and negate it.
    UnaryNeg,
    /// Pop a value and apply logical not (0 → 1, non-zero → 0).
    UnaryNot,
    /// Pop two values, apply a binary operator, and push the result.
    Binary(BinaryOp),
    /// Pop arguments and call a built-in function.
    CallBuiltin(BuiltinFn, usize),
    /// Pop an index and a container, then push the indexed value.
    Index,
    /// Pop arguments and a receiver, then call a method.
    CallMethod(String, usize),
    /// Pop a condition; if false, jump to the target instruction.
    JumpIfFalse(usize),
    /// Unconditional jump to the target instruction.
    Jump(usize),
    /// Begin a for-loop: pop iterable and set up iterator state.
    BeginFor(String),
    /// Advance iterator; if exhausted, jump to the end of the loop.
    CheckFor(String, usize),
    /// Pop a value and write it as an override for the target property.
    WriteOverride {
        /// Actor label to write the override to.
        target: String,
        /// Property name to override.
        property: String,
    },
    /// Halt execution.
    Halt,
}

/// A compiled modifier bytecode program.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModifierBytecodeProgram {
    /// Instruction sequence.
    pub instructions: Vec<Instruction>,
    /// Constant pool.
    pub constants: Vec<Value>,
}

/// Error during bytecode compilation.
#[derive(Clone, Debug, PartialEq)]
pub enum VmCompileError {
    /// Encountered an unsupported expression.
    UnsupportedExpr,
}

impl fmt::Display for VmCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmCompileError::UnsupportedExpr => {
                write!(
                    f,
                    "Modifier bytecode compiler encountered unsupported IR expression"
                )
            }
        }
    }
}

impl std::error::Error for VmCompileError {}

/// Compile a modifier IR program into bytecode.
pub fn compile_modifier_bytecode(
    program: &ModifierIrProgram,
) -> Result<ModifierBytecodeProgram, VmCompileError> {
    let mut compiler = BytecodeCompiler::default();
    for stmt in &program.statements {
        compiler.compile_stmt(stmt)?;
    }
    compiler.instructions.push(Instruction::Halt);
    Ok(ModifierBytecodeProgram {
        instructions: compiler.instructions,
        constants: compiler.constants,
    })
}

/// Execute a compiled modifier bytecode program.
pub fn execute_modifier_bytecode(
    program: &ModifierBytecodeProgram,
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
) -> Result<(), EvalError> {
    let mut vm = ModifierVm {
        stack: Vec::with_capacity(16),
        ip: 0,
        for_iteration_count: 0,
    };
    vm.run(program, frame_env, overrides)
}

#[derive(Default)]
struct BytecodeCompiler {
    instructions: Vec<Instruction>,
    constants: Vec<Value>,
}

impl BytecodeCompiler {
    fn add_const(&mut self, value: Value) -> usize {
        self.constants.push(value);
        self.constants.len() - 1
    }

    fn compile_stmt(&mut self, stmt: &ModifierIrStmt) -> Result<(), VmCompileError> {
        match stmt {
            ModifierIrStmt::Assign {
                target,
                property,
                value,
            } => {
                self.compile_modifier_expr(value)?;
                self.instructions.push(Instruction::WriteOverride {
                    target: target.join("."),
                    property: property.clone(),
                });
            }
            ModifierIrStmt::Let { name, value } => {
                self.compile_modifier_expr(value)?;
                self.instructions.push(Instruction::StoreEnv(name.clone()));
            }
            ModifierIrStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.compile_modifier_expr(condition)?;
                let jump_if_false_idx = self.instructions.len();
                self.instructions.push(Instruction::JumpIfFalse(usize::MAX));
                for stmt in then_branch {
                    self.compile_stmt(stmt)?;
                }
                let jump_idx = self.instructions.len();
                self.instructions.push(Instruction::Jump(usize::MAX));
                let else_start = self.instructions.len();
                for stmt in else_branch {
                    self.compile_stmt(stmt)?;
                }
                let end = self.instructions.len();
                self.instructions[jump_if_false_idx] = Instruction::JumpIfFalse(else_start);
                self.instructions[jump_idx] = Instruction::Jump(end);
            }
            ModifierIrStmt::For { var, iterable, body } => {
                self.compile_expr(iterable)?;
                self.instructions.push(Instruction::BeginFor(var.clone()));
                let check_idx = self.instructions.len();
                self.instructions.push(Instruction::CheckFor(var.clone(), usize::MAX));
                for stmt in body {
                    self.compile_stmt(stmt)?;
                }
                self.instructions.push(Instruction::Jump(check_idx));
                let end = self.instructions.len();
                self.instructions[check_idx] = Instruction::CheckFor(var.clone(), end);
            }
        }
        Ok(())
    }

    fn compile_modifier_expr(&mut self, expr: &ModifierExpr) -> Result<(), VmCompileError> {
        match expr {
            ModifierExpr::Compiled(expr) => self.compile_expr(expr),
            ModifierExpr::Unsupported(_) => Err(VmCompileError::UnsupportedExpr),
        }
    }

    fn compile_expr(&mut self, expr: &CompiledExpr) -> Result<(), VmCompileError> {
        match expr {
            CompiledExpr::Const(value) => {
                let idx = self.add_const(value.clone());
                self.instructions.push(Instruction::LoadConst(idx));
            }
            CompiledExpr::LoadEnv(name) => {
                self.instructions.push(Instruction::LoadEnv(name.clone()));
            }
            CompiledExpr::MakeVec(items) => {
                for item in items {
                    self.compile_expr(item)?;
                }
                self.instructions.push(Instruction::MakeVec(items.len()));
            }
            CompiledExpr::Unary(op, expr) => {
                self.compile_expr(expr)?;
                self.instructions.push(match op {
                    crate::ast::UnaryOp::Neg => Instruction::UnaryNeg,
                    crate::ast::UnaryOp::Not => Instruction::UnaryNot,
                });
            }
            CompiledExpr::Binary(left, op, right) => {
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.instructions.push(Instruction::Binary(op.clone()));
            }
            CompiledExpr::Select(condition, then_expr, else_expr) => {
                self.compile_expr(condition)?;
                let jump_if_false_idx = self.instructions.len();
                self.instructions.push(Instruction::JumpIfFalse(usize::MAX));
                self.compile_expr(then_expr)?;
                let jump_idx = self.instructions.len();
                self.instructions.push(Instruction::Jump(usize::MAX));
                let else_start = self.instructions.len();
                self.compile_expr(else_expr)?;
                let end = self.instructions.len();
                self.instructions[jump_if_false_idx] = Instruction::JumpIfFalse(else_start);
                self.instructions[jump_idx] = Instruction::Jump(end);
            }
            CompiledExpr::CallBuiltin(builtin, args) => {
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.instructions
                    .push(Instruction::CallBuiltin(builtin.clone(), args.len()));
            }
            CompiledExpr::Index(container, index) => {
                self.compile_expr(container)?;
                self.compile_expr(index)?;
                self.instructions.push(Instruction::Index);
            }
            CompiledExpr::Method(receiver, name, args) => {
                self.compile_expr(receiver)?;
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.instructions
                    .push(Instruction::CallMethod(name.clone(), args.len()));
            }
        }
        Ok(())
    }
}

struct ModifierVm {
    stack: Vec<Value>,
    ip: usize,
    /// Bounded iteration guard to prevent infinite loops in for-loops.
    for_iteration_count: usize,
}

impl ModifierVm {
    fn run(
        &mut self,
        program: &ModifierBytecodeProgram,
        frame_env: &mut Environment,
        overrides: &mut ModifierOverrides,
    ) -> Result<(), EvalError> {
        while self.ip < program.instructions.len() {
            match &program.instructions[self.ip] {
                Instruction::LoadConst(index) => {
                    let value = program.constants.get(*index)
                        .ok_or_else(|| EvalError::TypeMismatch(
                            format!("LoadConst index {} out of bounds (pool size {})", index, program.constants.len())
                        ))?;
                    self.stack.push(value.clone());
                    self.ip += 1;
                }
                Instruction::LoadEnv(name) => {
                    let value = frame_env
                        .get(name)
                        .ok_or_else(|| EvalError::UndefinedVariable(name.clone()))?;
                    self.stack.push(value);
                    self.ip += 1;
                }
                Instruction::StoreEnv(name) => {
                    let value = self.pop()?;
                    frame_env.set(name, value);
                    self.ip += 1;
                }
                Instruction::MakeVec(len) => {
                    let mut values = Vec::with_capacity(*len);
                    for _ in 0..*len {
                        values.push(self.pop()?);
                    }
                    values.reverse();
                    self.stack.push(make_vec_value(values));
                    self.ip += 1;
                }
                Instruction::UnaryNeg => {
                    let value = self.pop()?;
                    self.stack.push(Value::Num(-value.as_num()));
                    self.ip += 1;
                }
                Instruction::UnaryNot => {
                    let value = self.pop()?;
                    self.stack
                        .push(Value::Num(if value.as_num() == 0.0 { 1.0 } else { 0.0 }));
                    self.ip += 1;
                }
                Instruction::Binary(op) => {
                    let right = self.pop()?;
                    let left = self.pop()?;
                    self.stack.push(apply_binary_op(left, op, right)?);
                    self.ip += 1;
                }
                Instruction::CallBuiltin(builtin, arity) => {
                    let mut args = Vec::with_capacity(*arity);
                    for _ in 0..*arity {
                        args.push(self.pop()?);
                    }
                    args.reverse();
                    let result = match builtin {
                        BuiltinFn::Sin => eval_sin(&args),
                        BuiltinFn::Cos => eval_cos(&args),
                        BuiltinFn::Lerp => eval_lerp(&args),
                        BuiltinFn::Format => eval_format(&args),
                        BuiltinFn::Tan => eval_tan(&args),
                        BuiltinFn::Sqrt => eval_sqrt(&args),
                        BuiltinFn::Exp => eval_exp(&args),
                        BuiltinFn::Log => eval_log(&args),
                        BuiltinFn::Atan2 => eval_atan2(&args),
                        BuiltinFn::Clamp => eval_clamp(&args),
                        BuiltinFn::Abs => eval_abs(&args),
                        BuiltinFn::Min => eval_min(&args),
                        BuiltinFn::Max => eval_max(&args),
                        BuiltinFn::Floor => eval_floor(&args),
                        BuiltinFn::Ceil => eval_ceil(&args),
                    }?;
                    self.stack.push(result);
                    self.ip += 1;
                }
                Instruction::Index => {
                    let index_val = self.pop()?;
                    let container_val = self.pop()?;
                    let idx = index_val.as_num() as usize;
                    let result = match container_val {
                        Value::List(items) => items
                            .get(idx)
                            .cloned()
                            .ok_or_else(|| EvalError::TypeMismatch(format!(
                                "Index {} out of bounds for list of length {}",
                                idx,
                                items.len()
                            ))),
                        Value::Str(s) => s
                            .chars()
                            .nth(idx)
                            .map(|c| Value::Str(c.to_string()))
                            .ok_or_else(|| EvalError::TypeMismatch(format!(
                                "Index {} out of bounds for string of length {}",
                                idx,
                                s.len()
                            ))),
                        Value::Vec2(v) => match idx {
                            0 => Ok(Value::Num(v[0])),
                            1 => Ok(Value::Num(v[1])),
                            _ => Err(EvalError::TypeMismatch(format!(
                                "Index {} out of bounds for Vec2",
                                idx
                            ))),
                        },
                        Value::Vec3(v) => match idx {
                            0 => Ok(Value::Num(v[0])),
                            1 => Ok(Value::Num(v[1])),
                            2 => Ok(Value::Num(v[2])),
                            _ => Err(EvalError::TypeMismatch(format!(
                                "Index {} out of bounds for Vec3",
                                idx
                            ))),
                        },
                        Value::Vec4(v) => match idx {
                            0 => Ok(Value::Num(v[0])),
                            1 => Ok(Value::Num(v[1])),
                            2 => Ok(Value::Num(v[2])),
                            3 => Ok(Value::Num(v[3])),
                            _ => Err(EvalError::TypeMismatch(format!(
                                "Index {} out of bounds for Vec4",
                                idx
                            ))),
                        },
                        Value::Color(c) => match idx {
                            0 => Ok(Value::Num(c[0])),
                            1 => Ok(Value::Num(c[1])),
                            2 => Ok(Value::Num(c[2])),
                            3 => Ok(Value::Num(c[3])),
                            _ => Err(EvalError::TypeMismatch(format!(
                                "Index {} out of bounds for Color",
                                idx
                            ))),
                        },
                        other => Err(EvalError::TypeMismatch(format!(
                            "Cannot index into {:?}",
                            other
                        ))),
                    }?;
                    self.stack.push(result);
                    self.ip += 1;
                }
                Instruction::CallMethod(name, arity) => {
                    let mut args = Vec::with_capacity(*arity);
                    for _ in 0..*arity {
                        args.push(self.pop()?);
                    }
                    args.reverse();
                    let receiver = self.pop()?;
                    let result = super::ir::eval_method(receiver, name, &args)?;
                    self.stack.push(result);
                    self.ip += 1;
                }
                Instruction::JumpIfFalse(target) => {
                    if *target >= program.instructions.len() {
                        return Err(EvalError::TypeMismatch(
                            format!("JumpIfFalse target {} out of bounds ({} instructions)", target, program.instructions.len())
                        ));
                    }
                    let cond = self.pop()?;
                    if cond.as_num() == 0.0 {
                        self.ip = *target;
                    } else {
                        self.ip += 1;
                    }
                }
                Instruction::Jump(target) => {
                    if *target >= program.instructions.len() {
                        return Err(EvalError::TypeMismatch(
                            format!("Jump target {} out of bounds ({} instructions)", target, program.instructions.len())
                        ));
                    }
                    self.ip = *target;
                }
                Instruction::BeginFor(var) => {
                    self.for_iteration_count = 0;
                    let iterable = self.pop()?;
                    let items: Vec<Value> = match iterable {
                        Value::List(list) => list,
                        Value::Vec2(v) => v.into_iter().map(Value::Num).collect(),
                        Value::Vec3(v) => v.into_iter().map(Value::Num).collect(),
                        Value::Vec4(v) => v.into_iter().map(Value::Num).collect(),
                        other => vec![other],
                    };
                    frame_env.set(&format!("__for_iter_{var}"), Value::List(items));
                    frame_env.set(&format!("__for_idx_{var}"), Value::Num(0.0));
                    self.ip += 1;
                }
                Instruction::CheckFor(var, end) => {
                    self.for_iteration_count += 1;
                    if self.for_iteration_count > 100_000 {
                        return Err(EvalError::TypeMismatch(
                            "for-loop exceeded 100,000 iterations — possible infinite loop".to_string()
                        ));
                    }
                    let iter_key = format!("__for_iter_{var}");
                    let idx_key = format!("__for_idx_{var}");
                    let items = frame_env
                        .get(&iter_key)
                        .and_then(|v| match v { Value::List(l) => Some(l.clone()), _ => None })
                        .unwrap_or_default();
                    let idx = frame_env
                        .get(&idx_key)
                        .map(|v| v.as_num() as usize)
                        .unwrap_or(0);
                    if idx < items.len() {
                        frame_env.set(var, items[idx].clone());
                        frame_env.set(&idx_key, Value::Num((idx + 1) as f64));
                        self.ip += 1;
                    } else {
                        self.ip = *end;
                    }
                }
                Instruction::WriteOverride { target, property } => {
                    let value = self.pop()?;
                    overrides
                        .entry(target.clone())
                        .or_default()
                        .insert(property.clone(), value.clone());
                    crate::timeline::frame_env::apply_override_incremental(
                        frame_env, target, property, value,
                    );
                    self.ip += 1;
                }
                Instruction::Halt => break,
            }
        }
        Ok(())
    }

    fn pop(&mut self) -> Result<Value, EvalError> {
        self.stack
            .pop()
            .ok_or_else(|| EvalError::TypeMismatch("VM stack underflow".to_string()))
    }
}

impl fmt::Display for ModifierBytecodeProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (idx, instruction) in self.instructions.iter().enumerate() {
            match instruction {
                Instruction::LoadConst(const_idx) => {
                    writeln!(f, "{idx}: LoadConst {:?}", self.constants[*const_idx])?
                }
                Instruction::LoadEnv(name) => writeln!(f, "{idx}: LoadEnv {name}")?,
                Instruction::StoreEnv(name) => writeln!(f, "{idx}: StoreEnv {name}")?,
                Instruction::MakeVec(len) => writeln!(f, "{idx}: MakeVec {len}")?,
                Instruction::UnaryNeg => writeln!(f, "{idx}: UnaryNeg")?,
                Instruction::UnaryNot => writeln!(f, "{idx}: UnaryNot")?,
                Instruction::Binary(op) => writeln!(f, "{idx}: Binary {op:?}")?,
                Instruction::CallBuiltin(builtin, arity) => {
                    writeln!(f, "{idx}: CallBuiltin {builtin:?} {arity}")?
                }
                Instruction::Index => writeln!(f, "{idx}: Index")?,
                Instruction::CallMethod(name, arity) => {
                    writeln!(f, "{idx}: CallMethod {name} {arity}")?
                }
                Instruction::JumpIfFalse(target) => writeln!(f, "{idx}: JumpIfFalse {target}")?,
                Instruction::Jump(target) => writeln!(f, "{idx}: Jump {target}")?,
                Instruction::BeginFor(var) => writeln!(f, "{idx}: BeginFor {var}")?,
                Instruction::CheckFor(var, end) => writeln!(f, "{idx}: CheckFor {var} {end}")?,
                Instruction::WriteOverride { target, property } => {
                    writeln!(f, "{idx}: WriteOverride {target} {property}")?
                }
                Instruction::Halt => writeln!(f, "{idx}: Halt")?,
            }
        }
        Ok(())
    }
}

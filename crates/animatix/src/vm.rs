use crate::ast::BinaryOp;
use crate::ir::{
    BuiltinFn, CompiledExpr, ModifierExpr, ModifierIrProgram, ModifierIrStmt, ModifierOverrides,
    apply_binary_op, eval_cos, eval_format, eval_lerp, eval_sin, make_vec_value,
};
use crate::timeline::{Environment, EvalError, Value};
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Instruction {
    LoadConst(usize),
    LoadEnv(String),
    StoreEnv(String),
    MakeVec(usize),
    UnaryNeg,
    UnaryNot,
    Binary(BinaryOp),
    CallBuiltin(BuiltinFn, usize),
    JumpIfFalse(usize),
    Jump(usize),
    WriteOverride { target: String, property: String },
    Halt,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModifierBytecodeProgram {
    pub instructions: Vec<Instruction>,
    pub constants: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VmCompileError {
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

pub fn execute_modifier_bytecode<F>(
    program: &ModifierBytecodeProgram,
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
    mut refresh_env: F,
) -> Result<(), EvalError>
where
    F: FnMut(&mut Environment, &ModifierOverrides),
{
    let mut vm = ModifierVm {
        stack: Vec::new(),
        ip: 0,
    };
    vm.run(program, frame_env, overrides, &mut refresh_env)
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
        }
        Ok(())
    }
}

struct ModifierVm {
    stack: Vec<Value>,
    ip: usize,
}

impl ModifierVm {
    fn run<F>(
        &mut self,
        program: &ModifierBytecodeProgram,
        frame_env: &mut Environment,
        overrides: &mut ModifierOverrides,
        refresh_env: &mut F,
    ) -> Result<(), EvalError>
    where
        F: FnMut(&mut Environment, &ModifierOverrides),
    {
        while self.ip < program.instructions.len() {
            match &program.instructions[self.ip] {
                Instruction::LoadConst(index) => {
                    self.stack.push(program.constants[*index].clone());
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
                    }?;
                    self.stack.push(result);
                    self.ip += 1;
                }
                Instruction::JumpIfFalse(target) => {
                    let cond = self.pop()?;
                    if cond.as_num() == 0.0 {
                        self.ip = *target;
                    } else {
                        self.ip += 1;
                    }
                }
                Instruction::Jump(target) => {
                    self.ip = *target;
                }
                Instruction::WriteOverride { target, property } => {
                    let value = self.pop()?;
                    overrides
                        .entry(target.clone())
                        .or_default()
                        .insert(property.clone(), value);
                    refresh_env(frame_env, overrides);
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
                Instruction::JumpIfFalse(target) => writeln!(f, "{idx}: JumpIfFalse {target}")?,
                Instruction::Jump(target) => writeln!(f, "{idx}: Jump {target}")?,
                Instruction::WriteOverride { target, property } => {
                    writeln!(f, "{idx}: WriteOverride {target} {property}")?
                }
                Instruction::Halt => writeln!(f, "{idx}: Halt")?,
            }
        }
        Ok(())
    }
}

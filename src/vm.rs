use std::collections::HashMap;
use std::rc::Rc;
use crate::chunk::{Chunk, OpCode};
use crate::compiler::{Parser};
use crate::value::Value;
use crate::util::disassemble_instruction;

/*
When having an instruction like: print 3 - 2; in Lox is required to have instructions
for the operands 2 and 3, for the print and the plus. However, is not clear how the plus knows
that 2 is the minuend and 3 the subtrahend. A clearer example in the book is the following Lox code:
fun echo(n) {
  print n;
  return n;
}

print echo(echo(1) + echo(2)) + echo(echo(4) + echo(5));
The Lox semantics wants the result to be the following:

1  // from echo(1)
2  // from echo(2)
3  // from echo(1 + 2)
4  // from echo(4)
5  // from echo(5)
9  // from echo(4 + 5)
12 // from print 3 + 9

Rox was achieving this by recursively traversing the AST via a Post-Order Traversal, saving the left operand
on a Rust local (to the expression method) variable while traversing the right node.
In the bytecode however the AST is flattened in a series of instructions.
Observation: A value on the left of an operation remains available in order to be used in the operation
at least as the right operand and then can be "removed". Therefore, the first operand to be evaluated is the last
to be removed, or better the last operand to be evaluated is the first to be removed: LIFO, a Stack.
When an instruction produces a value it pushes it in the stack and when it evaluates it, it removes it from the stack.
 */
pub struct Vm {
    chunk: Chunk,
    ip: usize,
    stack: Vec<Value>,
    globals: HashMap<Rc<String>, Value>
}

pub enum Error {
    InterpretCompilerError,
    InterpretRuntimeError
}

enum ArithmeticOps {
    Add,
    Subtract,
    Multiply,
    Divide,
    Greater,
    Less
}

impl Vm {
    pub fn new(chunk: Chunk) -> Self {
        Vm {
            chunk,
            ip: 0,
            stack: Vec::new(),
            globals: HashMap::new()
        }
    }

    pub fn interpret(&mut self, source: String) -> Result<(), Error> {
        let mut parser = Parser::new(&source, &mut self.chunk);
        // compiler takes the empty chunk
        if !parser.compile() {
            return Err(Error::InterpretCompilerError);
        }
        self.run()
    }

    fn run(&mut self) -> Result<(), Error> {
        loop {
            #[cfg(feature = "debug-trace-execution")]
            {
                // use: cargo run --features debug-trace-execution
                print!("        ");
                for value in &self.stack {
                    print!("[{}]", value);
                }
                print!("\n");
                disassemble_instruction(&self.chunk, &self.ip);
            }

            // opcode of the bytecode instruction is read to understand what to do, process called 'dispatching'
            let instruction: u8 = self.read_byte();
            match OpCode::from_u8(instruction) {
                Some(OpCode::OpReturn) => {
                    /*if let Some(top_stack) = self.stack.pop() {
                        top_stack.print_value();
                    }
                    print!("\n");*/
                    return Ok(())
                },
                Some(OpCode::OpNegate) => {
                    match self.peek(0) {
                        Value::Number(number) => {
                            let value = Value::Number(-number);
                            self.stack.pop();
                            self.stack.push(value)
                        },
                        _ => return Err(self.runtime_error("Operand must be a number."))
                    }
                },
                Some(OpCode::OpAdd) => {
                    match (self.peek(0), self.peek(1)) {
                        // THIS NEEDS REFACTORING IN THE FUTURE, BECAUSE THERE IS NO STRING INTERNING
                        // CONSIDER MOVING THE INTERNING FROM COMPILER.RS TO CHUNK.RS BECAUSE IN THIS WAY
                        // THE HASHMAP WOULD BE ACCESSIBLE HERE IN THE VM.RS
                        (Value::String(a_val), Value::String(b_val)) => {
                            let ab = Value::String(Rc::new(format!("{}{}", b_val, a_val)));
                            self.stack.pop();
                            self.stack.pop();
                            self.stack.push(ab)
                        },
                        (Value::Number(_a_val), Value::Number(_b_val)) => self.binary_op(ArithmeticOps::Add)?,
                        _ => return Err(self.runtime_error("Operands must be two strings or two numbers."))
                    }
                },
                Some(OpCode::OpSubtract) => self.binary_op(ArithmeticOps::Subtract)?,
                Some(OpCode::OpMultiply) => self.binary_op(ArithmeticOps::Multiply)?,
                Some(OpCode::OpDivide) => self.binary_op(ArithmeticOps::Divide)?,
                Some(OpCode::OpNot) => {
                    if let Some(value) = self.stack.pop() {
                        self.stack.push(Value::Boolean(self.is_falsey(value)))
                    } else {
                        return Err(self.runtime_error("No value on the stack."))
                    }
                },
                Some(OpCode::OpConstant) => {
                    let constant = self.read_constant().clone();
                    self.stack.push(constant);
                },
                Some(OpCode::OpFalse) => self.stack.push(Value::Boolean(false)),
                Some(OpCode::OpEqual) => {
                    if let (Some(b), Some(a)) = (self.stack.pop(), self.stack.pop()) {
                        self.stack.push(Value::Boolean(self.values_equal(a, b)));
                    } else {
                        return Err(self.runtime_error("No values on the stack."))
                    }
                },
                Some(OpCode::OpPrint) => {
                    /*
                    At the time the interpreter reaches this point, the expression contained in the 'print' has already
                    been compiled and the result of the expression pushed on the stack.
                    An interesting thing to notice is that the Stack Effect of statements is zero, meaning that once the
                    statement has finished is net effect on the stack (values added to it) is zero differently from expression
                    which have a Stack Effect of one, regardless of how complex that expression is.
                     */
                    if let Some(stack_value) = self.stack.pop() {
                        stack_value.print_value();
                        print!("\n");
                    } else {
                        return Err(self.runtime_error("No value on the stack."))
                    }
                },
                Some(OpCode::OpGetGlobal) => {
                    let name = self.read_constant().clone();
                    match name {
                        Value::String(value) => {
                            if let Some(global_var) = self.globals.get(&value) {
                                self.stack.push(global_var.clone());
                            } else {
                                return Err(self.runtime_error(format!("undefined variable {}", value).as_str()))
                            }
                        },
                        _ => return Err(self.runtime_error("Variable name must be a string"))
                    }
                },
                Some(OpCode::OpGetLocal) => {
                    /*
                    When the OP_GET_LOCAL is executed, on the stack has been pushed the byte representing the index in the stack
                    where the local variable value is.
                    This index, is the same of the local variable name in the self.compiler.locals for the parser.
                     */
                    let slot = self.read_byte();
                    self.stack.push(self.stack[slot as usize].clone())
                },
                Some(OpCode::OpDefineGlobal) => {
                    let constant = self.read_constant().clone();
                    match constant {
                        Value::String(value) => {
                            self.globals.insert(value, self.peek(0).clone());
                            self.stack.pop();
                        },
                        _ => return Err(self.runtime_error("Variable name must be a string"))
                    }
                },
                Some(OpCode::OpSetGlobal) => {
                    let constant = self.read_constant().clone();
                    match constant {
                        Value::String(value) => {
                            if self.globals.get(&value).is_none() {
                                // in cases like:
                                // variable1 = "something"
                                // where variable1 has never been defined before e.g never done: var variable1;
                                return Err(self.runtime_error(format!("Undefined variable: {}", value).as_str()))
                            } else {
                                self.globals.insert(value, self.peek(0).clone());
                            }
                        },
                        _ => return Err(self.runtime_error("Variable name must be a string"))
                    }
                },
                Some(OpCode::OpSetLocal) => {
                    /*
                    In the same way, the cell of the stack at position slot needs to be equal to the top of the stack
                     */
                    let slot = self.read_byte();
                    self.stack[slot as usize] = self.peek(0).clone();
                },
                Some(OpCode::OpJumpIfFalse) => {
                    let offset = self.read_short();
                    // if the if condition is false, the ip is updated to proceed to the next instruction
                    // after the then_branch
                    if self.is_falsey(self.peek(0).clone()) {
                        self.ip += offset as usize;
                    }
                },
                Some(OpCode::OpJump) => {
                    let offset = self.read_short();
                    // it is an unconditional jump
                    self.ip += offset as usize;
                },
                Some(OpCode::OpLoop) => {
                    let offset = self.read_short();
                    // loop means going backward so we decrease
                    self.ip -= offset as usize;
                },
                Some(OpCode::OpPop) => { self.stack.pop(); },
                Some(OpCode::OpGreater) => self.binary_op(ArithmeticOps::Greater)?,
                Some(OpCode::OpLess) => self.binary_op(ArithmeticOps::Less)?,
                Some(OpCode::OpTrue) => self.stack.push(Value::Boolean(true)),
                Some(OpCode::OpNil) => self.stack.push(Value::Nil),
                None => ()
            }
        }
    }

    fn binary_op(&mut self, operation: ArithmeticOps) -> Result<(), Error> {
        return match (self.peek(0), self.peek(1)) {
            (Value::Number(b_ref), Value::Number(a_ref)) => {
                let b = *b_ref;
                let a = *a_ref;
                self.stack.pop();
                self.stack.pop();
                match operation {
                    ArithmeticOps::Add => self.stack.push(Value::Number(a + b)),
                    ArithmeticOps::Subtract => self.stack.push(Value::Number(a - b)),
                    ArithmeticOps::Multiply => self.stack.push(Value::Number(a * b)),
                    ArithmeticOps::Divide => self.stack.push(Value::Number(a / b)),
                    ArithmeticOps::Greater => self.stack.push(Value::Boolean(a > b)),
                    ArithmeticOps::Less => self.stack.push(Value::Boolean(a < b))
                }
                Ok(())
            },
            _ => Err(self.runtime_error("Operands must be numbers."))
        }
    }

    fn peek(&self, distance: usize) -> &Value {
        &self.stack[self.stack.len() - 1 -distance]
    }

    fn read_byte(&mut self) -> u8 {
        let bytecode = self.chunk.read(&self.ip);
        self.ip += 1;
        bytecode
    }

    fn read_short(&mut self) -> u16 {
        let first_byte = self.chunk.read(&self.ip);
        self.ip += 1;
        let second_byte = self.chunk.read(&self.ip);
        self.ip += 1;
        (first_byte | second_byte) as u16
    }

    fn read_constant(&mut self) -> &Value {
        let index = self.read_byte() as usize;
        self.chunk.get_constant(index)
    }

    fn values_equal(&self, a: Value, b: Value) -> bool {
        match (a, b) {
            (Value::Number(a_val), Value::Number(b_val)) => a_val == b_val,
            (Value::Boolean(a_val), Value::Boolean(b_val)) => a_val == b_val,
            (Value::String(a_val), Value::String(b_val)) => Rc::ptr_eq(&a_val, &b_val),
            (Value::Nil, Value::Nil) => true,
            (_, _) => false
        }
    }

    fn is_falsey(&self, value: Value) -> bool {
        match value {
            Value::Boolean(b) => !b,
            Value::Nil => true,
            _ => false
        }
    }

    fn runtime_error(&self, messages: &str) -> Error {
        eprintln!("{}", messages);
        let line = self.chunk.get_lines()[self.ip - 1];
        eprintln!("[line: {}] in script", line);
        Error::InterpretRuntimeError
    }
}


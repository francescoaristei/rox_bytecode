use std::collections::HashMap;
use std::rc::Rc;
use crate::chunk::Chunk;
use crate::chunk::OP_RETURN;
use crate::chunk::OP_CONSTANT;
use crate::chunk::OP_NEGATE;
use crate::chunk::OP_ADD;
use crate::chunk::OP_SUBTRACT;
use crate::chunk::OP_MULTIPLY;
use crate::chunk::OP_DIVIDE;
use crate::chunk::OP_TRUE;
use crate::chunk::OP_FALSE;
use crate::chunk::OP_NIL;
use crate::chunk::OP_NOT;
use crate::chunk::OP_EQUAL;
use crate::chunk::OP_GREATER;
use crate::chunk::OP_LESS;
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
            match instruction {
                OP_RETURN => {
                    if let Some(top_stack) = self.stack.pop() {
                        top_stack.print_value();
                    }
                    print!("\n");
                    return Ok(())
                },
                OP_NEGATE => {
                    if let Some(top_stack) = self.peek(0) {
                        match top_stack {
                            Value::Number(number) => {
                                let value = Value::Number(-number);
                                self.stack.pop();
                                self.stack.push(value)
                            },
                            _ => return Err(self.runtime_error("Operand must be a number."))
                        }
                    }
                },
                OP_ADD => {
                    if let (Some(a), Some(b)) = (self.peek(0), self.peek(1)) {
                        match (a, b) {
                            (Value::String(a_val), Value::String(b_val)) => {
                                let ab = Value::String(Rc::new(format!("{}{}", b_val, a_val)));
                                self.stack.pop();
                                self.stack.pop();
                                self.stack.push(ab)
                            },
                            (Value::Number(_a_val), Value::Number(_b_val)) => self.binary_op(ArithmeticOps::Add)?,
                            _ => return Err(self.runtime_error("Operands must be two strings or two numbers."))
                        }
                    } else {
                        return Err(self.runtime_error("No values on the stack."))
                    }
                },
                OP_SUBTRACT =>self.binary_op(ArithmeticOps::Subtract)?,
                OP_MULTIPLY =>self.binary_op(ArithmeticOps::Multiply)?,
                OP_DIVIDE =>self.binary_op(ArithmeticOps::Divide)?,
                OP_NOT => {
                    if let Some(value) = self.stack.pop() {
                        self.stack.push(Value::Boolean(self.is_falsey(value)))
                    } else {
                        return Err(self.runtime_error("No value on the stack."))
                    }
                },
                OP_CONSTANT => {
                    let constant = self.read_constant().clone();
                    self.stack.push(constant);
                },
                OP_FALSE => self.stack.push(Value::Boolean(false)),
                OP_EQUAL => {
                    if let (Some(b), Some(a)) = (self.stack.pop(), self.stack.pop()) {
                        self.stack.push(Value::Boolean(self.values_equal(a, b)));
                    } else {
                        return Err(self.runtime_error("No values on the stack."))
                    }
                },
                OP_GREATER => self.binary_op(ArithmeticOps::Greater)?,
                OP_LESS => self.binary_op(ArithmeticOps::Less)?,
                OP_TRUE => self.stack.push(Value::Boolean(true)),
                OP_NIL => self.stack.push(Value::Nil),
                _ => ()
            }
        }
    }

    fn binary_op(&mut self, operation: ArithmeticOps) -> Result<(), Error> {
        if let (Some(b_ref), Some(a_ref)) = (self.peek(0), self.peek(1)) {
            return match (b_ref, a_ref) {
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
        Err(self.runtime_error("Not enough values in the stack."))
    }

    fn peek(&self, distance: usize) -> Option<&Value> {
        self.stack.get(self.stack.len() - 1 -distance)
    }

    fn read_byte(&mut self) -> u8 {
        let bytecode = self.chunk.read(&self.ip);
        self.ip += 1;
        bytecode
    }

    fn read_constant(&mut self) -> &Value {
        let index = self.read_byte() as usize;
        self.chunk.get_constant(index)
    }

    fn values_equal(&self, a: Value, b: Value) -> bool {
        match (a, b) {
            (Value::Number(a_val), Value::Number(b_val)) => a_val == b_val,
            (Value::Boolean(a_val), Value::Boolean(b_val)) => a_val == b_val,
            (Value::String(a_val), Value::String(b_val)) => a_val == b_val,
            (Value::Nil, Value::Nil) => true,
            (_, _) => false
        }
    }

    fn is_falsey(&self, value: Value) -> bool {
        match value {
            Value::Boolean(bool) => bool,
            Value::Nil => false,
            _ => true
        }
    }

    fn runtime_error(&self, messages: &str) -> Error {
        eprintln!("{}", messages);
        let line = self.chunk.get_lines()[self.ip - 1];
        eprintln!("[line: {}] in script", line);
        Error::InterpretRuntimeError
    }
}


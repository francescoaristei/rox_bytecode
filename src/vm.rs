use crate::chunk::Chunk;
use crate::chunk::OP_RETURN;
use crate::chunk::OP_CONSTANT;
use crate::chunk::OP_NEGATE;
use crate::chunk::OP_ADD;
use crate::chunk::OP_SUBTRACT;
use crate::chunk::OP_MULTIPLY;
use crate::chunk::OP_DIVIDE;
use crate::value::Value;

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
    stack: Vec<Value>
}

pub enum InterpretResult {
    InterpretOk,
    InterpretCompileError,
    InterpretRuntimeError
}

impl Vm {
    pub fn new(chunk: Chunk) -> Self {
        Vm {
            chunk,
            ip: 0,
            stack: Vec::new()
        }
    }

    pub fn run(&mut self) -> InterpretResult {
        loop {
            #[cfg(feature = "debug-trace-execution")]
            {
                // use: cargo run --features debug-trace-execution
                print!("        ");
                for value in &self.stack {
                    print!("[{}]", value);
                }
                print!("\n");
                self.chunk.disassemble_instruction(&self.chunk, &self.ip);
            }

            // opcode of the bytecode instruction is read to understand what to do, process called 'dispatching'
            let instruction: u8 = self.read_byte();
            match instruction {
                OP_RETURN => {
                    if let Some(top_stack) = self.stack.pop() {
                        top_stack.print_value();
                    }
                    print!("\n");
                    return InterpretResult::InterpretOk
                },
                OP_NEGATE => {
                    if let Some(top_stack) = self.stack.pop() {
                        match top_stack {
                            Value::Number(number) => self.stack.push(Value::Number(-number))
                        }
                    }
                },
                OP_ADD => self.binary_op("ADD"),
                OP_SUBTRACT => self.binary_op("SUB"),
                OP_MULTIPLY => self.binary_op("MUL"),
                OP_DIVIDE => self.binary_op("DIV"),
                OP_CONSTANT => {
                    let constant = self.read_constant().clone();
                    self.stack.push(constant);
                }
                _ => ()
            }
        }
    }

    fn binary_op(&mut self, operation: &str) {
        if let (Some(b), Some(a)) = (self.stack.pop(), self.stack.pop()) {
            match (b, a) {
                (Value::Number(b), Value::Number(a)) => {
                    match operation {
                        "ADD" => self.stack.push(Value::Number(a + b)),
                        "SUB" => self.stack.push(Value::Number(a - b)),
                        "MUL" => self.stack.push(Value::Number(a * b)),
                        "DIV" => self.stack.push(Value::Number(a / b)),
                        _ => () // TO-DO: ADD ERROR HANDLING
                    }
                }
            }
        }
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
}


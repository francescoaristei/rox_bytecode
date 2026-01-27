use std::cell::Cell;
use std::collections::HashMap;
use crate::chunk::{Chunk, OpCode};
use crate::compiler::{Parser};
use crate::heap::{Heap, ObjRef};
use crate::value::Value;
use crate::util::disassemble_instruction;
use crate::value::Value::Obj;
use crate::heap::HeapObj;


/*
* Frame/Base Pointer:
* How should the local variables of each function be allocated on the stack?
* Each function statically assigned a set of stack at compile time?
* Each function call at runtime a new piece of stack memory allocated?
* The values in the stack continue to behave in a LIFO fashion also
* when considering in the stack the local variables of each function
* in the code.
* Given the following example:
*
* fun first() {
*   var a = 1;
*   second();
*   var b = 2;
* }
*
* fun second() {
*   var c = 3;
*   var d = 4;
}
first();
*
* The stack allocation of the variables is:
* a,
* a, c, d (c and d get destroyed)
* a, b
* -> LIFO!
* It would be efficient to know at compile time where the variables for each function
* should go on the stack, however knowing the absolute location is not easy:
* fun first() {
*   var a = 1;
*   second();
*   var b = 2;
*   second();
* }
*
* fun second() {
*   var c = 3;
*   var d = 4;
* }
*
* first();
*
*
* Now in ghe first call to second() c and d go into 1 and 2 slots, while in the second
* call they go into 2 and 3 slots in the stack.
* What however is always possible to know is the position of a local variable relative to the
* beginning of the function it belongs to.
* At the beginning of each function call, the VM records the location of the first slot where
* the function locals begin.
* The instruction working with the locals access them using the index relative to that location,
* not the absolute one.
* At compile time the compiler computes these relative indexes which are translated into absolute
* indexes on the stack at runtime.
* The name of the start of a function locals is called: frame pointer or base pointer.
*
* Return Address:
* The VM goes through the bytecode, incrementing the instruction pointer everytime an instruction
* is completed. When a function completes, the VM needs to jump back to the instruction following the
* invocation of that function. The address to which it has to jump back is what it is called the
* return address.
* This property is not of the function itself, but of the invocation of the function, as with recursive
* calls there could be more return addresses for the same function.
* A CallFrame describes a function invocation.
* The ip is the address of the calling instruction which, when calling the function stores its location
* in the function (CallFrame) fields.
*
*
 */
pub struct CallFrame {
    function: ObjRef,
    ip: Cell<usize>,
    slots: usize
}


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
    // The function calls (call frame) also have a natural stack behavior -> calling funcA, funcB
    // we have that funcB is removed from the stack before funcA (which was called earlier).
    frames: Vec<CallFrame>,
    stack: Vec<Value>,
    globals: HashMap<ObjRef, Value>,
    heap: Heap
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
    pub fn new() -> Self {
        Vm {
            frames: Vec::new(),
            stack: Vec::new(),
            globals: HashMap::new(),
            heap: Heap::new()
        }
    }

    pub fn interpret(&mut self, source: String) -> Result<(), Error> {
        let mut parser = Parser::new(&source, &mut self.heap);
        let function = parser.compile();
        if function.is_none() {
            return Err(Error::InterpretCompilerError);
        }
        let obj_ref = self.heap.alloc(HeapObj::Function(function.unwrap()));
        self.stack.push(Obj(obj_ref));
        self.frames.push(
            CallFrame {
                function: obj_ref,
                ip: Cell::new(0),
                slots: 0
            }
        );
        self.run()
    }

    fn run(&mut self) -> Result<(), Error> {
        loop {
            #[cfg(feature = "debug-trace-execution")]
            {
                // use: cargo run --features debug-trace-execution
                print!("        ");
                for value in &self.stack {
                    print("[");
                    value.print_value(&self.heap);
                    print!("]");
                }
                print!("\n");
                let chunk = self.get_current_chunk();
                let ip = self.current_frame().ip.get();
                disassemble_instruction(chunk, &ip, &self.heap);
            }

            // opcode of the bytecode instruction is read to understand what to do, process called 'dispatching'
            let instruction: u8 = self.read_byte();
            match OpCode::from_u8(instruction) {
                Some(OpCode::OpReturn) => {
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
                    let a = self.peek(0);
                    let b = self.peek(1);
                    match (a.as_string(&self.heap), b.as_string(&self.heap)) {
                        (Some(a_str), Some(b_str)) => {
                            let ab = format!("{}{}", b_str, a_str);
                            let handle = self.heap.intern_string(ab);
                            self.stack.pop();
                            self.stack.pop();
                            self.stack.push(Value::Obj(handle))
                        },
                        _ => {
                            match (a, b) {
                                (Value::Number(_a_val), Value::Number(_b_val)) => self.binary_op(ArithmeticOps::Add)?,
                                _ => return Err(self.runtime_error("Operands must be two strings or two numbers."))
                            }
                        }
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
                        stack_value.print_value(&self.heap);
                        print!("\n");
                    } else {
                        return Err(self.runtime_error("No value on the stack."))
                    }
                },
                Some(OpCode::OpGetGlobal) => {
                    let name = self.read_constant();
                    match name {
                        Obj(obj_ref) => {
                            if let Some(global_var) = self.globals.get(obj_ref) {
                                self.stack.push(*global_var);
                            } else {
                                let name = name.as_string(&self.heap).expect("Expected global string.");
                                return Err(self.runtime_error(&format!("undefined variable {}", name)))
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
                    let last = self.frames.len() - 1;
                    let frame = &self.frames[last];
                    let slot = self.read_byte();
                    // the local variable in the call frame is at the position given by the beginning of the
                    // call frame in the stack (frame.slots) + the relative position inside the call frame (slot)
                    self.stack.push(self.stack[frame.slots + slot as usize].clone())
                },
                Some(OpCode::OpDefineGlobal) => {
                    let constant = self.read_constant();
                    match constant {
                        Obj(obj_ref) => {
                            self.globals.insert(*obj_ref, *self.peek(0));
                            self.stack.pop();
                        },
                        _ => return Err(self.runtime_error("Variable name must be a string"))
                    }
                },
                Some(OpCode::OpSetGlobal) => {
                    let constant = self.read_constant();
                    match constant {
                        Obj(obj_ref) => {
                            if self.globals.get(&obj_ref).is_none() {
                                // in cases like:
                                // variable1 = "something"
                                // where variable1 has never been defined before e.g never done: var variable1;
                                let name = constant.as_string(&self.heap).expect("Expected global string.");
                                return Err(self.runtime_error(&format!("Undefined variable: {}", name)))
                            } else {
                                self.globals.insert(*obj_ref, *self.peek(0));
                            }
                        },
                        _ => return Err(self.runtime_error("Variable name must be a string"))
                    }
                },
                Some(OpCode::OpSetLocal) => {
                    /*
                    In the same way, the cell of the stack at position slot needs to be equal to the top of the stack
                     */
                    let last = self.frames.len() - 1;
                    let frame = &self.frames[last];
                    let slot = self.read_byte();
                    self.stack[frame.slots + slot as usize] = self.peek(0).clone();
                },
                Some(OpCode::OpJumpIfFalse) => {
                    let offset = self.read_short();
                    // if the if condition is false, the ip is updated to proceed to the next instruction
                    // after the then_branch
                    if self.is_falsey(self.peek(0).clone()) {
                        let frame = self.current_frame();
                        frame.ip.set(frame.ip.get() + offset as usize);
                    }
                },
                Some(OpCode::OpJump) => {
                    let offset = self.read_short();
                    let frame = self.current_frame();
                    // it is an unconditional jump
                    frame.ip.set(frame.ip.get() + offset as usize);
                },
                Some(OpCode::OpLoop) => {
                    let offset = self.read_short();
                    let frame = self.current_frame();
                    // loop means going backward so we decrease
                    frame.ip.set(frame.ip.get() - offset as usize);
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
        &self.stack[self.stack.len() - 1 - distance]
    }

    fn get_current_chunk(&self) -> &Chunk {
        let last = self.frames.len() - 1;
        let frame = &self.frames[last];
        match self.heap.get(frame.function) {
            HeapObj::Function(obj_function) => &obj_function.chunk,
            _ => panic!("Expected function")
        }
    }

    fn current_frame(&self) -> &CallFrame {
        &self.frames[self.frames.len() - 1]
    }

    fn read_byte(&self) -> u8 {
        let chunk = self.get_current_chunk();
        let frame = self.current_frame();
        let ip = frame.ip.get();
        let bytecode = chunk.read(&ip);
        frame.ip.set(ip + 1);
        bytecode
    }

    fn read_short(&self) -> u16 {
        let first = self.read_byte() as u16;
        let second = self.read_byte() as u16;
        (first << 8) | second
    }

    fn read_constant(&self) -> &Value {
        let index = self.read_byte() as usize;
        self.get_current_chunk().get_constant(index)
    }

    fn values_equal(&self, a: Value, b: Value) -> bool {
        match (a, b) {
            (Value::Number(a_val), Value::Number(b_val)) => a_val == b_val,
            (Value::Boolean(a_val), Value::Boolean(b_val)) => a_val == b_val,
            (Obj(a_ref), Obj(b_ref)) => a_ref == b_ref,
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
        let frame = self.current_frame();
        let chunk = self.get_current_chunk();
        let line = chunk.get_lines()[frame.ip.get() - 1];
        eprintln!("[line: {}] in script", line);
        Error::InterpretRuntimeError
    }
}


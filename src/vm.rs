use crate::chunk::{Chunk, OpCode};
use crate::compiler::Parser;
use crate::heap::HeapObj;
use crate::heap::{Heap, ObjRef};
use crate::value::Value;
use crate::value::Value::Obj;
use std::cell::Cell;
use std::collections::HashMap;
use crate::object::{NativeFn, ObjClosure, ObjNativeFunction, ObjUpvalue};
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
    closure: ObjRef,
    //function: ObjRef,
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
    heap: Heap,
    upvalues: Option<ObjRef>
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
            heap: Heap::new(),
            upvalues: None
        }
    }

    // example of native function
    fn clock_native(_arg_count: u8, _args: &[Value]) -> Value {
        let duration = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        Value::Number(duration.as_secs_f64())
    }

    pub fn interpret(&mut self, source: String) -> Result<(), Error> {
        self.define_native("clock", Self::clock_native);
        let mut parser = Parser::new(&source, &mut self.heap);
        let function = parser.compile();
        if function.is_none() {
            return Err(Error::InterpretCompilerError);
        }
        /*
         * Compiler roots problem.
         *
         * The compiler allocates strings on the VM heap during compilation (via heap.intern_string()),
         * storing ObjRefs in the function's constant pool and name field. These strings are only
         * referenced from the ObjFunction, which at this point is still a Rust local variable —
         * not yet on the VM heap, and therefore invisible to the GC.
         *
         * When alloc(Function) is called, debug-gc mode triggers a collection *before* the function
         * is placed on the heap. The GC cannot see the Rust-local ObjFunction, so it has no way to
         * reach the strings in its constant pool. Those strings are swept, leaving the function
         * with dangling ObjRefs.
         *
         * The fix: before calling alloc(Function), push all ObjRefs the function holds (its name
         * and every Obj constant) onto the VM stack as temporary roots. The GC traverses the stack
         * during mark_roots(), so these strings will be kept alive. Once the function is safely on
         * the heap and itself rooted, we pop the temporaries.
         *
         * In normal execution this is harmless — GC won't trigger this early given the 1MB
         * threshold — but it makes debug-gc stress mode correct too.
         *
         * Note: in clox the equivalent fix is markCompilerRoots(), which walks the live compiler
         * chain and marks each in-progress ObjFunction. Here compilation is already finished by
         * the time we call alloc(), so pushing constants directly is simpler.
         */
        let function = function.unwrap();
        let temp_root_count = {
            let constants = function.chunk.get_constants().get_values();
            let obj_constants: Vec<ObjRef> = constants.iter().filter_map(|v| v.as_obj()).collect();
            let count = obj_constants.len() + function.name.is_some() as usize;
            for obj_ref in obj_constants {
                self.stack.push(Obj(obj_ref));
            }
            if let Some(name_ref) = function.name {
                self.stack.push(Obj(name_ref));
            }
            count
        };
        let obj_function_ref = self.alloc(HeapObj::Function(function));
        for _ in 0..temp_root_count {
            self.stack.pop();
        }
        self.stack.push(Obj(obj_function_ref)); // temp root: keeps the Function reachable if GC runs inside alloc(Closure)
        let closure = ObjClosure::new(obj_function_ref);
        let obj_function_closure = self.alloc(HeapObj::Closure(closure));
        self.stack.pop();
        self.stack.push(Obj(obj_function_closure));
        self.call(obj_function_closure, 0);
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
                Some(OpCode::OpCloseUpValue) => {
                    self.close_upvalues(self.stack.len() - 1);
                    self.stack.pop();
                },
                /*
                * Peeks the top of the stack Value.
                * - Upvalue open: stores Value in the stack at the location: frame.closure.upvalues[slot].location
                * - Upvalue closed: stores Value in the closed field of the upvalue: frame.closure.upvalues[slot]
                 */
                Some(OpCode::OpSetUpValue) => {
                    let slot = self.read_byte();
                    let value = *self.peek(0);
                    let last = self.frames.len() - 1;
                    let frame = &self.frames[last];
                    let obj_upvalue = self.heap.get(frame.closure).as_closure().obj_upvalues[slot as usize];
                    let upvalue = self.heap.get_mut(obj_upvalue).as_upvalue_mut();
                    if upvalue.closed.is_none() {
                        self.stack[upvalue.location] = value;
                    } else {
                        upvalue.closed = Some(value);
                    }
                },
                /*
                * Reads the operand byte as an upvalue index.
                * Looks up the corresponding upvalue in the current closure upvalues array.
                * Upvalue:
                * - Open: Pushes onto the stack the value at the stack position upvalue.location.
                * - Closed: Pushes onto the stack the value stored in the closed field of the upvalue.
                 */
                Some(OpCode::OpGetUpValue) => {
                    // reads the operand byte as an upvalue index
                    let slot = self.read_byte();

                    let last = self.frames.len() - 1;
                    let frame = &self.frames[last];

                    // looks up the corresponding upvalue in the current closure's upvalue array
                    let upvalue_ref = self.heap.get(frame.closure).as_closure().obj_upvalues[slot as usize];
                    let upvalue = self.heap.get(upvalue_ref).as_upvalue();

                    if let Some(closed) = upvalue.closed {
                        self.stack.push(closed)
                    } else {
                        self.stack.push(self.stack[upvalue.location]);
                    }
                }
                Some(OpCode::OpClosure) => {
                    let constant = self.read_constant().clone();
                    match constant {
                        Obj(obj_ref) => {
                            let upvalue_count = self.heap.get(obj_ref).as_function().upvalue_count;
                            let obj_closure = ObjClosure::new(obj_ref);
                            /*
                            * Need to push the Function's ObjRef onto the stack before allocating the Closure, to keep it
                            * rooted during GC — then pop it after. This is the same technique clox uses.
                            * Inside the OpClosure dispatch, where a function constant is wrapped in a closure at runtime.
                             */
                            self.stack.push(Obj(obj_ref)); // temp root: keeps the Function reachable if GC runs inside alloc
                            let closure_ref = self.alloc(HeapObj::Closure(obj_closure));
                            self.stack.pop();
                            self.stack.push(Obj(closure_ref));

                            /*
                              The compiler writes, the VM reads.

                              Compiler side — when compiling a function declaration, after the function body is compiled:
                              OP_CLOSURE
                              <constant index>     ← points to the ObjFunction in the constants table
                              <is_local> <index>   ← pair for upvalue 0
                              <is_local> <index>   ← pair for upvalue 1
                              ...                  ← one pair per upvalue

                              The compiler iterates over its compile-time upvalue array and serializes each one as two bytes into the bytecode.
                              This makes OP_CLOSURE a variable-length instruction — its size
                              depends on how many upvalues the function captures.

                              VM side — when executing OP_CLOSURE
                              The VM reads the constant index, gets the ObjFunction, creates an ObjClosure.
                              Then it needs to populate the closure's runtime upvalue array. It knows how many pairs to read from
                              ObjFunction.upvalue_count.
                              For each pair, it reads the two bytes the compiler wrote and uses them
                              to either create a new runtime upvalue (pointing to a stack slot) or share an
                              existing one from the enclosing closure
                              Concrete example with:
                              fun outer() {
                                  var x = 1;
                                  var y = 2;
                                  fun inner() { print x; print y; }
                              }
                              The compiler emits:
                              OP_CLOSURE  <const index of inner>
                              1  3        ← upvalue 0: is_local=true, index=3 (x's stack slot in outer)
                              1  4        ← upvalue 1: is_local=true, index=4 (y's stack slot in outer)

                              The VM reads those same bytes back and builds two runtime ObjUpvalue objects pointing to stack slots 3 and 4.
                              The closure's obj_upvalues vec ends up as [ref_to_upvalue_for_x, ref_to_upvalue_for_y].
                              Later, when inner executes OP_GET_UPVALUE 0, the VM looks up obj_upvalues[0],
                              follows it to the runtime upvalue, and reads the value from the stack (or from closed if it's been
                              closed).
                            *
                             */
                            let mut upvalues = Vec::new();
                            for _ in 0..upvalue_count {
                                let is_local = self.read_byte();
                                let index = self.read_byte();
                                if is_local == 1 {
                                    /*
                                    *
                                    * The OP_CLOSURE is emitted at the end of a function declaration.
                                    * At the moment the declaration is being done, the current function
                                    * is the enclosing one. This means that the current function closure is
                                    * stored in the top of the frame stack (current frame).
                                    *
                                     */
                                    upvalues.push(self.capture_upvalue(self.current_frame().slots + index as usize));
                                } else {
                                    let closure = self.heap.get(self.current_frame().closure).as_closure();
                                    upvalues.push(closure.obj_upvalues[index as usize]);
                                }
                            }
                            self.heap.get_mut(closure_ref).as_closure_mut().obj_upvalues = upvalues;
                        },
                        _ => panic!("Expect a function as constant.")
                    }
                },
                /*
                * When a function returns, the return value is on the top of the stack.
                * - The return value is popped from the stack. A
                * - The call frame of the returning function is discarded. B
                * - If it was the most-outer function (the script) then we return. C
                * - Otherwise, all the slots used by the returning function for local variables etc are discarded. D
                * - The returned value is pushed again onto the stack. E
                * - The frame is now the one of the caller, at the next iteration the instruction pointer will point to it.
                *
                * |------------------------------------| --> top level code
                *
                * script    4   sum     5   6   7   18
                *
                *              |-----------------| --> function sum()
                *
                * B,C,D
                * |------------------------------------| --> top level code
                *
                * script    4   sum     5   6   7                           popped --> 18
                *
                *              |-----------------| --> function sum()
                *
                *
                * B,C,D
                * |------------------------------------| --> top level code
                *
                * script    4                                               popped --> 18
                *
                *
                * E
                * |------------------------------------| --> top level code
                *
                * script    4   18 <-- pushed
                *
                *
                *
                 */
                Some(OpCode::OpReturn) => {
                    let result = self.stack.pop();
                    let frame = self.frames.pop().expect("Expect frame of the returning function.");
                    /*
                      OP_RETURN case

                      fun outer() {
                          var x = "hello";
                          fun inner() { print x; }
                          return inner;
                      }

                      var fn = outer();
                      fn();

                      When outer() returns, x is still a top-level local of outer — no block has ended,
                      so OP_CLOSE_UPVALUE was never emitted for it. But outer's entire call frame is about to be
                      discarded by OP_RETURN. The close_upvalues(frame.slots) call inside OP_RETURN catches x's upvalue
                      and moves "hello" into it before the stack is truncated. Without this, fn()
                      would read from a dead stack slot.

                      OP_CLOSE_UPVALUE case

                      fun outer() {
                          var closure;
                          {
                              var x = "hello";
                              fun inner() { print x; }
                              closure = inner;
                          }
                          // x is gone here, but closure still needs it
                          closure();
                      }

                      outer();

                      Here x is declared inside a block within outer.
                      When the } is reached, x goes out of scope — but outer hasn't returned yet.
                      The compiler emits OP_CLOSE_UPVALUE for x at the end
                      of that block. This closes the upvalue mid-function, moving "hello" into it.
                      When closure() is called two lines later, the upvalue is already closed and carries its own value.

                      If you only had the OP_RETURN path, the upvalue for x would remain open.
                      The stack slot for x gets popped at }, and by the time closure() runs, that slot has been reused or is
                      invalid.
                    *
                     */
                    self.close_upvalues(frame.slots);
                    if self.frames.is_empty() {
                        self.stack.pop();
                        return Ok(())
                    }
                    self.stack.truncate(frame.slots);
                    self.stack.push(result.expect("Expected value returned from the function."));
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
                            let handle = self.intern_string(ab);
                            self.stack.pop();
                            self.stack.pop();
                            self.stack.push(Obj(handle))
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
                /*
                * Given the following code:
                * fun sum(a, b, c) {
                *  return a + b + c;
                * }
                *
                * print 4 + sum(5, 6, 7);
                *
                * At the time of the OP_CALL the stack looks like this:
                *
                * [script, 4, sum, 5, 6, 7]
                *
                * When the function its compiled, it's local 0 slot is
                * allocated for the function name, then the slots for the
                * parameters are allocated.
                * Each function also needs to have a CallFrame associated with it.
                * It is important to collect the arguments with which the function is
                * called, and put them in the already allocated slots of the function (for the parameters).
                * For how the stack is populated, this is achieved "for free", meaning that this is the stack:
                * 0     1       2       3       4       5 <- indexes of the top level caller
                * [..   ..     [sum     5       6       7]] the parentheses define the global vs local stack
                *               0       1       2       3 <- indexes of sum(), the callee
                *
                * Is free to bind an argument to a parameter.
                 */
                Some(OpCode::OpCall) => {
                    let arg_count = self.read_byte();
                    let value = self.peek(arg_count as usize);
                    if !self.call_value(*value, arg_count) {
                        return Err(self.runtime_error("Runtime error while calling function."))
                    }
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

    /*
    * The linked list of upvalues is created in such a way to be sorted
    * by descending slot index.
    * Where the slot is the slot of the closed over variable on the stack
    * the upvalue points to/contains.
    * Consider this Lox code:
    *
    *  fun outer() {
    *      var a = 1;  // slot 1
    *      var b = 2;  // slot 2
    *      var c = 3;  // slot 3
    *
    *      fun inner() { print a; print c; }
    *  }
    *
    *  When inner captures a and c, the VM creates upvalues and inserts them into the linked list.
    *  The list is maintained sorted by slot index descending, so:
    *
    *  head -> [slot 3] -> [slot 1] -> null
    *
    *  Now suppose a different closure wants to capture b (slot 2). The VM walks the list:
    *
    *  1. Look at head: slot 3 — that's above slot 2, keep walking.
    *  2. Look at next: slot 1 — that's below slot 2.
    *
    *  At this point you stop. You know slot 2 isn't in the list because you've passed the point where it would be.
    *  If it existed, it would be between slot 3 and slot 1. You don't need
    *  to check the rest of the list. You insert a new upvalue for slot 2 right here:
    *
    *  head -> [slot 3] -> [slot 2] -> [slot 1] -> null
    *
    *  Without sorting, you'd have to walk the entire list every time to confirm an upvalue doesn't exist.
    *  The same logic applies to close_upvalues(slot).
    *  When a scope ends (say at slot 2), you walk from the head and close every upvalue with location >= 2.
    *  The moment you hit one with location < 2, you stop — everything after it is guaranteed to be even lower.
    *
    *
     */
    fn capture_upvalue(&mut self, index: usize) -> ObjRef {
        let mut prev_upvalue: Option<ObjRef> = None;
        let mut upvalue = self.upvalues;
        while upvalue.is_some() {
            let upvalue_object = self.heap.get(upvalue.unwrap()).as_upvalue();
            if upvalue_object.location <= index {
                break;
            }
            prev_upvalue = upvalue;
            upvalue = upvalue_object.next;
        }

        if upvalue.is_some() {
            let upvalue_object = self.heap.get(upvalue.unwrap()).as_upvalue();
            // the local slot we stopped at is the one we are looking for
            // an existing upvalue containing the closed over variable
            // exists and can be returned
            if upvalue_object.location == index {
                return upvalue.unwrap();
            }
        }
        let created_upvalue_objref = self.alloc(HeapObj::Upvalue(ObjUpvalue::new(index)));
        self.heap.get_mut(created_upvalue_objref).as_upvalue_mut().next = upvalue;

        if prev_upvalue.is_none() {
            self.upvalues = Some(created_upvalue_objref);
        } else {
            self.heap.get_mut(prev_upvalue.unwrap()).as_upvalue_mut().next = Some(created_upvalue_objref);
        }
        created_upvalue_objref
    }

    /*
    *
    * When a local variable goes out of scope (popped from the stack), any open upvalue pointing at
    * that stack slot would become a dangling reference. close_upvalues(last) prevents
    * this by walking the linked list and, for every open upvalue with location >= last,
    * copying the stack value into the upvalue's closed field. From that point on, the upvalue is
    * "closed" — it carries its own copy of the value and no longer depends on the stack.
    *
     */
    fn close_upvalues(&mut self, last: usize) {
        while self.upvalues.is_some() {
            let upvalue_object = self.heap.get_mut(self.upvalues.unwrap()).as_upvalue_mut();
            if upvalue_object.location < last {
                break
            }
            upvalue_object.closed = Some(self.stack[upvalue_object.location]);
            self.upvalues = upvalue_object.next;
        }
    }

    /*
    * When this is called, on the stack (local stack) there is on the first slot
    * the name of the function, beneath all the arguments needed by it.
    * Using the arg_count is therefore possible to access the name of the function
     */
    fn call_value(&mut self, value: Value, arg_count: u8) -> bool {
        match value {
            Obj(obj_ref) => {
                match self.heap.get(obj_ref) {
                    //HeapObj::Function(_) => self.call(obj_ref, arg_count),
                    HeapObj::Closure(_) => { self.call(obj_ref, arg_count) },
                    HeapObj::NativeFunction(native_fn) => {
                        let result = (native_fn.fun)(arg_count, &self.stack[arg_count as usize..self.stack.len()]);
                        self.stack.truncate(arg_count as usize + 1);
                        self.stack.push(result);
                        true
                    },
                    _ => false
                }
            },
            _ => {
                self.runtime_error("Can only call functions and classes.");
                false
            }
        }
    }

    /*
    * It initializes the frame of the called function
    * pushing it into the stack of frames.
     */
    fn call(&mut self, obj_ref: ObjRef, arg_count: u8) -> bool {
        let closure = self.heap.get(obj_ref).as_closure();
        let arity = self.heap.get(closure.obj_function).as_function().arity;

        if arity != arg_count as usize {
            self.runtime_error(&format!("Expected {} arguments but got {}", arity, arg_count));
            return false;
        }

        // NOTE: I AVOIDED CHECKING ON THE MAXIMUM STACK FRAMES SIZE

        let frame = CallFrame {
            //function: obj_ref,
            closure: obj_ref,
            ip: Cell::new(0),
            // the start of the slots for that frame
            // the -1 is because the slot 0 is reserved for methods by the compiler.
            slots: self.stack.len() - (arg_count as usize) - 1
        };
        self.frames.push(frame);
        true
    }

    fn binary_op(&mut self, operation: ArithmeticOps) -> Result<(), Error> {
        match (self.peek(0), self.peek(1)) {
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
        let closure = self.heap.get(frame.closure).as_closure();
        &self.heap.get(closure.obj_function).as_function().chunk
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

    fn define_native(&mut self, name: &str, fun: NativeFn) {
        // intern the name of the native function and get the obj ref
        let name_handle = self.intern_string(name.to_string());
        // allocate the native function on the heap and get the obj ref
        let obj_native = ObjNativeFunction::new(fun);
        let fun_handle = self.alloc(HeapObj::NativeFunction(obj_native));
        // insert in the globals as key the obj ref of the interned string and as key the Obj having
        // value the obj ref of the native function on the heap
        self.globals.insert(name_handle, Obj(fun_handle));
    }

    fn alloc(&mut self, object: HeapObj) -> ObjRef {
        #[cfg(feature = "debug-gc")]
        {
            self.collect_garbage();
        }

        #[cfg(feature = "debug-log-gc")]
        let type_name = object.type_name();

        if self.heap.get_bytes_allocated() > self.heap.get_next_gc() {
            self.collect_garbage();
        }

        let obj_ref = self.heap.alloc(object);

        #[cfg(feature = "debug-log-gc")]
        println!("{:?} allocate {}", obj_ref, type_name);

        obj_ref
    }

    fn intern_string(&mut self, string: String) -> ObjRef {
        self.heap.intern_string(string)
    }

    /*
    * Garbage Collector
    * The garbage collector (gc) is that part of the compiler which cleans automatically memory when
    * possible without having the programming language (Lox) think about it.
    * In order for the gc to do so, it needs to know which memory is possible to free.
    * Every value defined in a program falls under two categories:
    *   - reachable
    *   - unreachable
    * A value is reachable if there is some way for the user program to 'reference' it.
    * The work of the gc is to understand which value is not reachable anymore by the
    * executing program and to deallocate it, freeing memory.
    * Every value can be divided further into two categories:
    *   - root
    *   - non-root
    * A root value, is any object that the VM can reach without having to go through a reference in
    * some other object.
    * Examples of root objects are: local variables, global variables (accessible from the
    * global table), variables sitting on the stack.
    * To understand non-root cases lets see an example.
    *
    * fun makeClosure() {
    *   var a = "data";
    *
    *    fun f() { print a; }
    *      return f;
    *     }
    *
    * {
    *   var closure = makeClosure();
    *   // GC here.
    *   closure();
    *  }
    *
    * When executed the program prints "data".
    * The "data" string is not on the stack.
    * To access it is the following:
    * ObjClosure -> upvalues -> upvalues[0] -> ObjUpvalue -> closed -> ObjString -> "data"
    *
    * Variable closure is on the stack, but to get to the string, is required to pass through its
    * upvalues array.
    * Therefore, also "data" is reachable.
    * It follows an inductive definition of reachability:
    *   - All roots are reachable
    *   - Any object referred by a reachable object is also reachable
    *
    * Any object which does not fall into this definition, is not reachable and is duty of the gc
    * to remove it from memory.
    * The basic idea of the algorithm is straightforward:
    *   - Traverse all objects starting from the roots and put them in a set
    *   * All objects not in that set need to be freed.
    *
    * Mark and Sweep
    *   - Traverse the graph of all objects starting from the roots one and 'mark' them.
    *   - Once the traversal is completed, sweep all objects not marked.
    *
     */
    fn collect_garbage(&mut self) {
        #[cfg(feature = "debug-log-gc")]
        let before = self.heap.get_bytes_allocated();

        #[cfg(feature = "debug-log-gc")]
        println!("--gc begin");

        self.mark_roots();
        self.heap.trace_references();
        self.heap.clear_strings();
        self.heap.sweep();

        // next_gc always has a minimum of 1024 * 1024 otherwise if gets to 0, then is always kept to 0 (0 * 2 = 0) and gc happens forever.
        self.heap.set_next_gc((self.heap.get_bytes_allocated() * 2).max(1024 * 1024));

        #[cfg(feature = "debug-log-gc")]
        println!("--gc end");

        #[cfg(feature = "debug-log-gc")]
        println!(
            "   collected {} bytes (from {} to {}) next at {}",
            before - self.heap.get_bytes_allocated(),
            before,
            self.heap.get_bytes_allocated(),
            self.heap.get_next_gc()
        );
    }

    /*
    * This implementation miss the markCompilerRoots() used in clox.
    * This happens because heap-allocated objects are accessible only from the vm
    * therefore is not possible to move the GC trigger in the heap itself.
    * First compilation happens, then execution via VM and here is where GC operates.
    * Therefore, the objects allocated by the compiler (string interning, ObjFunction allocated
    * for each function being compiled) do not risk of being removed by the GC during compilation.
    * However, this is not a problem as every object allocated by the compiler, referenced by the
    * compiled program, ends up being cleaned by the GC once it becomes unreachable.
    *
     */
    fn mark_roots(&mut self) {
        for value in &self.stack {
            /*
            * Stack and global memory have deterministic lifetimes managed by the runtime
            * Stack variables are allocated and freed automatically as functions are called and freed.
            * Global variables need to live for the whole lifetime of the program.
            * Heap memory is dynamically allocated at runtime with no predictable lifetime, the gc
            * therefore, tracks which heap objects are still reachable and frees the ones that are not.
             */
            if let Some(obj_ref) = value.as_obj() {
                self.heap.mark_object(obj_ref)
            }
        }

        /*
        * The VM mantains a separate stack of callframes, each call frame contains points to the closure being called
        * The VM uses these pointers to access constants and upvalues, therefore the closures needs to be kept as well.
         */
        for frame in &self.frames {
            self.heap.mark_object(frame.closure)
        }

        // also the open upvalues are objects that are directly reachable by the VM
        let mut upvalue = self.upvalues;
        while let Some(upvalue_ref) = upvalue {
            self.heap.mark_object(upvalue_ref);
            let upvalue_obj = self.heap.get(upvalue_ref).as_upvalue();
            upvalue = upvalue_obj.next;
        }

        self.mark_globals();
    }

    fn mark_globals(&mut self) {
        for obj_ref in self.globals.keys() {
            self.heap.mark_object(*obj_ref);
            if let Some(obj_ref) = self.globals[obj_ref].as_obj() {
                self.heap.mark_object(obj_ref);
            }
        }
    }

    fn runtime_error(&self, messages: &str) -> Error {
        eprintln!("{}", messages);
        // the call stack is shown from the most recently called
        // function to the first called function
        for frame in self.frames.iter().rev() {
            let closure = self.heap.get(frame.closure).as_closure();
            let obj_function = self.heap.get(closure.obj_function).as_function();
            let line = obj_function.chunk.get_lines()[frame.ip.get() - 1];
            eprint!("[line {}] in ", line);
            match obj_function.name {
                Some(name_ref) => {
                    eprintln!("{}()", self.heap.get(name_ref).as_string().chars)
                },
                None => eprintln!("script")
            }
        }
        let frame = self.current_frame();
        let chunk = self.get_current_chunk();
        let line = chunk.get_lines()[frame.ip.get() - 1];
        eprintln!("[line: {}] in script", line);
        Error::InterpretRuntimeError
    }
}


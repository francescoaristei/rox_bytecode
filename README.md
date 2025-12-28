## Bytecode Interpreter


### Notes

The advantages of the Tree Walk Interpreter written are:

- Simple to implement: direct translation from runtime representation of the code to the syntax.
- Portable on any platform.

A problem with the Tree-Walk interpreter solution is spatial locality.
Taking for example the binary expression used:

```
Binary {
    id: usize,
    left: Box<Expr>,
    operator: Token,
    right: Box<Expr>
}
```

The left and right operands are pointers to heap-allocated data. Both operands are recursively Expr which can add
more data on the heap.
This adds a 32 or 64 bit of overhead for the pointers, plus, having all these data scattered around the heap can be costly
as it could require accesses in RAM instead that on caches.

Compiling directly to machine code is fast but not portable, as it would require a different implementation for each
different instruction sets.

The `bytecode` solution used, is something in between: uses higher-level constructs without requiring to write machine code,
but is far more efficient than the Tree-walk interpreter.

The bytecode itself is resembling the machine code, in the sense that is composed by a set of binary instructions, however
much simpler and at a higher-level than actual machine code.
Such instructions do not really exists as instruction sets for any architecture, therefore to make them execute on any given
hardware platform is required to write an `emulator` or better said, a `virtual machine`: basically an emulated chip
that takes as input the bytecode instructions and translates them in the respective low-level machine instructions that the architecture
where the code is compiled is able to understand.

The use of the virtual machine is the reason for the portability of the bytecode solution and at the same time of it being
less efficient than the native code implementation of the compiler.

The parallel with the Tree-walk interpreter can be represented like this:

```

                jlox                               clox
        ┌────────────────┐                ┌────────────────┐
        │     Parser     │ ←─ FrontEnd ─→ │    Compiler    │
        └────────────────┘                └────────────────┘
                 │                                  │
                 ▼                                  ▼
        ┌────────────────┐                ┌────────────────┐
        │  Syntax Trees  │ ← Representation → │   Bytecode   │
        │     (AST)      │                │  0x326A        │
        └────────────────┘                │  0x1752        │
                 │                        │  0x60B8        │
                 ▼                        └────────────────┘
        ┌────────────────┐                ┌────────────────┐
        │  Interpreter   │ ←─ Execution─→ │ Virtual Machine│
        └────────────────┘                └────────────────┘

```

Why BYTECODE IS BETTER THAN TREE WALK INTERPRETER: NOTES

VEC<> FOR STACK:

How Vec<T> Memory Layout Works

A Vec<T> is actually stored in two places:

The Vec Struct Itself (Metadata)

pub struct Vec<T> {
ptr: *mut T,      // Pointer to heap buffer
len: usize,       // Current number of elements
cap: usize,       // Allocated capacity
}
// Size: 24 bytes on 64-bit systems

This metadata lives wherever the Vec is defined.

The Actual Elements (Data)

The elements are always on the heap, accessed through the pointer.

In Your VM

pub struct Vm {
chunk: Chunk,
ip: usize,
stack: Vec<Value>,  // ← Let's analyze this
}

When you create a VM:
let mut vm = Vm::new(chunk);

Memory layout:
Stack:
┌─────────────────┐
│ Vm struct       │
│  - chunk: ...   │
│  - ip: usize    │
│  - stack:       │
│    - ptr   ───────────┐
│    - len       │      │
│    - cap       │      │
└─────────────────┘      │
│
Heap:                    │
┌────────────────────────┘
│
▼
┌─────────────────────────┐
│ Vec buffer:             │
│ [Value, Value, Value...] │
└─────────────────────────┘

- Vm struct: On the stack (local variable)
- Vec metadata (ptr, len, cap): Inside Vm struct, so also on stack
- Vec buffer (the actual Value elements): On the heap

Why This Matters

1. Cache Friendliness

The Vec buffer is a contiguous block of heap memory:
// When you push values:
vm.stack.push(Value::Number(1.0));
vm.stack.push(Value::Number(2.0));
vm.stack.push(Value::Number(3.0));

Memory layout on heap:
[Value::Number(1.0) | Value::Number(2.0) | Value::Number(3.0)]
↑ Contiguous, sequential, cache-friendly

This is much better than scattered AST nodes:
Heap (scattered):
Address 0x1000: Box(Expr::Literal)
Address 0x3500: Box(Expr::Binary)
Address 0x7200: Box(Expr::Literal)
↑ Random locations, cache-unfriendly

2. Stack Operations Are Fast

let b = self.stack.pop();   // O(1) - just decrement len
let a = self.stack.pop();   // O(1)
self.stack.push(result);    // O(1) amortized

All operations just modify:
- The heap buffer (read/write a single memory location)
- The length field (increment/decrement)

No pointer chasing, no indirection.

3. Comparison with AST Traversal

Tree-walk (your jlox in Rust):
// Each Box<Expr> is a separate heap allocation
fn evaluate(&mut self, expr: Box<Expr>) -> Value {
match *expr {
Expr::Binary { left, op, right } => {
let left = self.evaluate(left);   // ← Dereference Box, jump to random heap location
let right = self.evaluate(right); // ← Another random heap location
// ...
}
}
}

Bytecode:
loop {
match self.read_byte() {  // ← Sequential bytecode array
OP_ADD => {
let b = self.stack.pop();  // ← Sequential stack array
let a = self.stack.pop();
self.stack.push(a + b);
}
}
}

WHY BYTECODE MORE EFFICIENT IN MEMORY AND TIME:

Performance Advantages

1. Execution Speed

Tree-walk interpreter (your first implementation):
fn visit_binary(&mut self, left: Box<Expr>, op: Token, right: Box<Expr>) -> Value {
let left_val = self.evaluate(*left);   // Recursive call
let right_val = self.evaluate(*right); // Recursive call
match op.token_type {
TokenType::Plus => /* ... */
}
}
- Every operation requires method calls, pattern matching, heap allocations
- Deeply nested expressions = deep call stack
- Cache-unfriendly: AST nodes scattered in memory

Bytecode VM:
loop {
match self.read_byte() {
OP_ADD => {
let b = self.stack.pop();
let a = self.stack.pop();
self.stack.push(a + b);
}
}
}
- Tight loop, sequential memory access
- Values on stack (Vec), very cache-friendly
- No recursion overhead
- ~3-10x faster typically (though Rust narrows this gap vs C)

2. Memory Efficiency

AST representation of 1 + 2 * 3:
Binary {
left: Box(Literal(1)),           // ~48+ bytes
op: Token::Plus,                 // ~32+ bytes  
right: Box(Binary {              // ~48+ bytes
left: Box(Literal(2)),       // ~48+ bytes
op: Token::Star,             // ~32+ bytes
right: Box(Literal(3))       // ~48+ bytes
})
}
// Total: ~256+ bytes (rough estimate, includes enum tags, padding)

Bytecode representation:
OP_CONSTANT 0    // 2 bytes (1 + 2 * 3)
OP_CONSTANT 1    // 2 bytes
OP_CONSTANT 2    // 2 bytes
OP_MULTIPLY      // 1 byte
OP_ADD           // 1 byte
// Total: 8 bytes of instructions + constant pool

Result: Bytecode is 10-30x more compact than AST in memory.



> My question is, in case of local variables, how can I access the identifier for a certain variable? like a for var a? If the set puts in the stack in the position where 'a'
is expected its value, where I have access to the actual identnfier of the variable?

● Excellent question! This reveals the key insight of how local variables work.

The Answer: You Don't Need the Identifier at Runtime!

The variable name (like "a") is only needed during compilation. At runtime, the VM never sees or uses the name - it only uses the slot number.

Where the Identifier Lives

At Compile Time

compiler.rs:93-105:
struct Local<'a> {
name: Token<'a>,      // ← The identifier "a" is HERE
depth: Option<usize>
}

The compiler maintains compiler.locals: Vec<Local>, which stores:
- locals[0].name = Token("a")
- locals[1].name = Token("b")
- etc.

When You Write a = 5

Step 1: Compiler sees identifier "a"

compiler.rs:468-480 (resolve_local):
fn resolve_local(&mut self) -> Option<u8> {
// Search through all locals
for i in self.compiler.local_count - 1..0 {
if let Some(local) = self.compiler.locals.get(i as usize) {
// Compare the identifier we're looking up with stored names
if std::ptr::eq(local.name.get_lexeme(), self.previous.get_lexeme()) {
//            ^^^^^^^^^^^^^^^^^^^^      ^^^^^^^^^^^^^^^^^^^^^^^^
//            stored name "a"           current identifier "a"
if local.depth.is_none() {
self.error("Can't read local variable in its own initializer.");
}
return Some(i);  // ← Returns 0 (the slot number)
}
}
}
None
}

Step 2: Compiler emits bytecode with only the slot number:
OpConstant 5
OpSetLocal 0    ← Just the number 0, not the name "a"!

At Runtime - The Name Disappears!

vm.rs:224-230:
Some(OpCode::OpSetLocal) => {
let slot = self.read_byte();  // Reads 0 from bytecode
// No access to "a" - we only have the number 0!
self.stack[slot as usize] = self.peek(0).expect("...").clone();
}

The VM never knows the variable was called "a". It just knows "slot 0".

Visual Timeline

┌─────────────────────────────────────────────────────────────┐
│ COMPILE TIME                                                 │
├─────────────────────────────────────────────────────────────┤
│ Source code: var a = 1;                                     │
│                                                              │
│ Compiler state:                                             │
│   compiler.locals = [                                       │
│     Local { name: Token("a"), depth: Some(1) }              │
│   ]                                                          │
│                                                              │
│ When seeing: a = 5                                          │
│   1. resolve_local() searches locals by name                │
│   2. Finds "a" at index 0                                   │
│   3. Emits: OpSetLocal 0  ← Only the index!                 │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ RUNTIME                                                      │
├─────────────────────────────────────────────────────────────┤
│ Bytecode: OpSetLocal 0                                      │
│                                                              │
│ VM state:                                                   │
│   stack = [1, 2, 5]                                         │
│   (no variable names stored!)                               │
│                                                              │
│ Execution:                                                  │
│   1. Read slot = 0 from bytecode                            │
│   2. stack[0] = stack.top()  // stack[0] = 5                │
│   3. Done! Never needed the name "a"                        │
└─────────────────────────────────────────────────────────────┘

Why This Works

The compiler does the "name → slot number" translation once during compilation. By the time the code runs, all variable references have been converted to slot numbers.

Compile-time mapping:

"a" → slot 0
"b" → slot 1
"c" → slot 2

Runtime bytecode (no names!):

OpGetLocal 0    // Not "OpGetLocal a"
OpSetLocal 1    // Not "OpSetLocal b"

Contrast with Global Variables

| Aspect                  | Local Variables         | Global Variables               |
  |-------------------------|-------------------------|--------------------------------|
| Name stored at runtime? | ❌ No                   | ✅ Yes (in HashMap)            |
| Bytecode operand        | Slot number (u8)        | Index to constant table → name |
| Runtime lookup          | stack[slot]             | globals.get(name)              |
| Speed                   | Very fast (array index) | Slower (hash lookup)           |

Global Variable Example:

// Compile time for: globalVar = 5
OpConstant 5
OpSetGlobal 0    // 0 = index in constant table
// constants[0] = String("globalVar")

// Runtime
Some(OpCode::OpSetGlobal) => {
let name_index = self.read_byte();  // 0
let name = self.read_constant();     // String("globalVar")
// Now we have the name and can do: globals.insert(name, value)
}

Summary

Where is the identifier "a"?
- Compile time: In compiler.locals[0].name
- Runtime: Nowhere! It's been compiled away to just the number 0

This is what makes local variables fast - the name resolution happens once during compilation, then at runtime it's just array indexing with no string comparisons or hash lookups needed.

## Bytecode Interpreter


### Usage

#### Build

```bash
cargo build
```

#### Run a Lox script

```bash
cargo run -- path/to/script.lox
```

#### Interactive REPL

```bash
cargo run
```

Launches a prompt where you can type Lox expressions line by line. Exit with `Ctrl+Z` (Windows) or `Ctrl+D` (Unix).

#### Debug modes

Run with bytecode disassembly and execution tracing enabled:

```bash
cargo run --features debug-trace-execution -- path/to/script.lox
```

#### Debugging the Garbage Collector

Two feature flags control GC diagnostics, and can be combined freely.

**`debug-log-gc`** — logs every heap allocation and the full lifecycle of each GC cycle:

```bash
cargo run --features debug-log-gc -- path/to/script.lox
```

Sample output:

```
ObjRef(0) allocate String
ObjRef(1) allocate Function
--gc begin
ObjRef(0) mark String
ObjRef(0) blacken String
ObjRef(1) mark Function
ObjRef(1) blacken Function
--gc end
   collected 128 bytes (from 256 to 128) next at 256
```


**`debug-gc`** — stress-tests the GC by triggering a collection on **every single allocation** instead of waiting for the heap threshold to be reached. 
Useful for catching GC correctness bugs that would otherwise only surface under memory pressure:

```bash
cargo run --features debug-gc -- path/to/script.lox
```

Both flags can be combined to get stress-test runs with full logging:

```bash
cargo run --features debug-gc,debug-log-gc -- path/to/script.lox
```


### Notes

The advantages of the Tree Walk Interpreter written are:

- Simple to implement: direct translation from runtime representation of the code to the syntax.
- Portable on any platform.

A problem with the Tree-Walk interpreter solution is spatial locality.
Taking for example the binary expression used:

```rust
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
where the code is compiled is able to understand -> this is possible using the compiler of the language in which the VM is implemented:
Rust in this case.

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
        │  Syntax Trees  │ ← Represent  → │  Bytecode      │
        │     (AST)      │                │  0x326A        │
        └────────────────┘                │  0x1752        │
                 │                        │  0x60B8        │
                 ▼                        └────────────────┘
        ┌────────────────┐                ┌────────────────┐
        │  Interpreter   │ ←─ Execution─→ │ Virtual Machine│
        └────────────────┘                └────────────────┘
```

#### How `Vec<T>` Memory Layout Works

A `Vec<T>` is actually stored in two places:

##### The Vec Struct Itself (Metadata)

```rust
pub struct Vec<T> {
    ptr: *mut T,      // Pointer to heap buffer
    len: usize,       // Current number of elements
    cap: usize,       // Allocated capacity
}
```

The size is 24 bytes on 64-bit systems. This metadata lives wherever the Vec is defined.

##### The Actual Elements (Data)

The elements are always on the heap, accessed through the pointer.

##### In the VM

```rust
pub struct Vm {
    chunk: Chunk,
    ip: usize,
    stack: Vec<Value>
}
```

When the VM is created with `let mut vm = Vm::new(chunk);`, the memory layout is the following:

```
Stack:
┌─────────────────┐
│ Vm struct       │
│  - chunk: ...   │
│  - ip: usize    │
│  - stack:       │
│    - ptr ───────────────────┐
│    - len        │           │
│    - cap        │           │
└─────────────────┘           │
                              │
Heap:                         │
                              ▼
┌─────────────────────────┐
│ Vec buffer:             │
│ [Value, Value, Value...]│
└─────────────────────────┘
```

- **Vm struct**: On the stack (local variable)
- **Vec metadata** (ptr, len, cap): Inside Vm struct, so also on stack.
- **Vec buffer** (the actual Value elements): On the heap.

The advantages of it are the following:

##### 1. Cache Friendliness

The Vec buffer is a contiguous block of heap memory:

```rust
vm.stack.push(Value::Number(1.0));
vm.stack.push(Value::Number(2.0));
vm.stack.push(Value::Number(3.0));
```

Memory layout on heap:

```
[Value::Number(1.0) | Value::Number(2.0) | Value::Number(3.0)]
↑ Contiguous, sequential, cache-friendly
```

This is much better than scattered AST nodes:

```
Heap (scattered):
Address 0x1000: Box(Expr::Literal)
Address 0x3500: Box(Expr::Binary)
Address 0x7200: Box(Expr::Literal)
↑ Random locations, cache-unfriendly
```

##### Stack Operations Are Fast

```rust
let b = self.stack.pop();   // O(1) - just decrement len
let a = self.stack.pop();   // O(1)
self.stack.push(result);    // O(1)
```

All operations just modify:
- The heap buffer (read/write a single memory location)
- The length field (increment/decrement)

No pointer chasing, no indirection.


In the Tree-Walk Interpreter each `Box<Expr>` is a separate heap allocation.

```rust
fn evaluate(&mut self, expr: Box<Expr>) -> Value {
    match *expr {
        Expr::Binary { left, op, right } => {
        let left = self.evaluate(left);   // ← Dereference Box, jump to random heap location
        let right = self.evaluate(right); // ← Another random heap location
...
        }
    }
}
```

Bytecode:

```rust
loop {
    match self.read_byte() {  // ← Sequential bytecode array
        OP_ADD => {
            let b = self.stack.pop();  // ← Sequential stack array
            let a = self.stack.pop();
            self.stack.push(a + b);
        }
    }
}
```

#### Why the Bytecode implementation is more efficient in memory and time?

##### Performance Advantages

###### 1. Execution Speed

Tree-walk interpreter:

```rust
fn visit_binary(&mut self, left: Box<Expr>, op: Token, right: Box<Expr>) -> Value {
    let left_val = self.evaluate(*left);   // Recursive call
    let right_val = self.evaluate(*right); // Recursive call
    match op.token_type {
        TokenType::Plus => /* ... */
    }
}
```

- Every operation requires method calls, pattern matching, heap allocations
- Deeply nested expressions = deep call stack
- Cache-unfriendly: AST nodes scattered in memory

Bytecode VM:

```rust
loop {
    match self.read_byte() {
        OP_ADD => {
            let b = self.stack.pop();
            let a = self.stack.pop();
            self.stack.push(a + b);
        }
    }
}
```

- Tight loop, sequential memory access
- Values on stack (Vec), very cache-friendly
- No recursion overhead


AST representation of `1 + 2 * 3`:

```rust
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
```

Bytecode representation:

```
OP_CONSTANT 0    // 2 bytes (1 + 2 * 3)
OP_CONSTANT 1    // 2 bytes
OP_CONSTANT 2    // 2 bytes
OP_MULTIPLY      // 1 byte
OP_ADD           // 1 byte
// Total: 8 bytes of instructions + constant pool
```

Result: Bytecode is 10-30x more compact than AST in memory.

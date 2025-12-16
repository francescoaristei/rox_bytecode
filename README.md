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
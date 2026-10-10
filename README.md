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
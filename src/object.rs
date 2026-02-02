use crate::chunk::Chunk;
use crate::heap::ObjRef;
use crate::value::Value;

pub struct ObjString {
    pub chars: String
}

#[derive(Clone, Copy, PartialEq)]
pub enum FunctionType {
    TypeFunction,
    TypeScript
}

/*
* Each function has its own chunk. Whenever the compiler reaches a function declaration
* it has to compile its body and save it into the function's chunk and then return to the
* chunk containing the compiled code in which the function is contained.
 */
pub struct ObjFunction {
    pub arity: usize,
    pub chunk: Chunk,
    pub name: Option<ObjRef>,
    pub upvalue_count: usize
}

impl ObjFunction {
    pub fn new() -> Self {
        ObjFunction {
            arity: 0,
            chunk: Chunk::new(),
            name: None,
            upvalue_count: 0
        }
    }
}

// function pointer for native functions
pub type NativeFn = fn(arg_count: u8, args: &[Value]) -> Value;

/*
* In order to do stuff a programming language implementation needs
* native functions (to read file system, read user input etc).
* These are called from Lox but implemented in the language used
* to implement Lox.
* Native functions don't call a CallFrame because there is no bytecode
* that frame could point to, they reference native (Rust in this case) code.
 */
pub struct ObjNativeFunction {
    pub name: Option<ObjRef>,
    pub fun: NativeFn
}

impl ObjNativeFunction {
    pub fn new(native_fn: NativeFn) -> Self {
        ObjNativeFunction {
            name: None,
            fun: native_fn
        }
    }
}
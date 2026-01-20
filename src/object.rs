use crate::chunk::Chunk;
use crate::heap::ObjRef;

pub struct ObjString {
    pub chars: String
}

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
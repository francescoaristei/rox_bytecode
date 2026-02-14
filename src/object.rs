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

/*
* In clox Closures are implemented such that variables that are not part of closures
* keep living on the stack, which makes them very efficient,
* while for variables which are part of closures, these will be moved onto the heap
* in order to be able to be referenced by different function closures during their lifetime
* (which will be longer than the usual runtime of local variables)
*
* Why Closures require runtime checks?
* Consider the following code:
*
* fun makeClosure(value) {
*  fun closure() {
*    print value;
*  }
*  return closure;
* }
*
* var doughnut = makeClosure("doughnut");
* var bagel = makeClosure("bagel");
* doughnut();
* bagel();
*
* And then consider this:
*
*  fun printValue(value) {
*    print value;
*  }
*
*  printValue("doughnut");
*  printValue("bagel");
*
* In the cas  with 'parameters' the value is passed at call time
* With 'parameters' closure() is compiled once as a static bloc of bytecode.
* At runtime the caller pushes arguments onto the stack and the function
* reads them from the stack slots.
*
* In case of closures, the value is captured at creation time.
* When doughnut() is called, the caller passes 0 arguments, "doughnut" was put into
* the function when makeClosure("doughnut") executed, the returned function remembers
* the environment it was in.
* With closure the bytecode must somehow reference a variable that:
* - does not exist in its scope
* - come from makeClosure scope, which has returned
* - has different values ("doughnut" vs "bagel") even though they share the same bytecode.
* => each closure needs runtime data ("doughnut" or "bagel") attached to the same compiled bytecode
*
*
* The way closures are implemented is to have a struct wrapping the compile time function which "closes"
* the closure will have in addition the runtime values of the variables it closes over.
 */

pub struct ObjClosure {
    pub obj_function: ObjRef, // reference to the function on the heap
    pub obj_upvalues: Vec<ObjRef>
}

impl ObjClosure {
    pub fn new(obj_function: ObjRef) -> Self {
        ObjClosure {
            obj_function,
            obj_upvalues: Vec::new()
        }
    }
}

pub struct CompilerUpvalue {
    pub index: u8,
    pub is_local: bool
}

impl CompilerUpvalue {
    pub fn new(index: u8, is_local: bool) -> Self {
        CompilerUpvalue {
            index,
            is_local
        }
    }
}


/*
* Runtime view of the Upvalue
* When an upvalue contains a closed-over variable which is still
* on the stack (not out of scope yet) it is called an open upvalue.
* When instead the variable goes out of scope and gets removed from
* the stack, its value is moved inside the upvalue (on the heap) and
* the upvalue becomes a closed upvalue.
 */
pub struct ObjUpvalue {
    pub location: usize,
    /*
    * The list of all the upvalues is accessed by the VM in order to:
    * - When a function (having an upvalue) closes over a variable,
    *   which has to be moved to the heap because it goes out of scope (removed from stack)
    *   the VM tries to identify if there is already another upvalue object
    *   who closed over that variable and shares the reference to the other upvalue
    *   This is important as closures close over variables (not values) therefore
    *   if two closures close over the same variable, mutation on the variable need
    *   to be seen by both closures.
    *
    * - When a variable goes out of scope and gets removed from the stack, the VM iterates
    *   over the list to check all the (open) upvalues that needs to become closed ones
    *   and get the value of the descoped variable.
     */
    pub next: Option<ObjRef>,
    pub closed: Option<Value>
}

impl ObjUpvalue {
    pub fn new(location: usize) -> Self {
        ObjUpvalue {
            location,
            next: None,
            closed: None
        }
    }
}
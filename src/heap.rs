use std::collections::HashMap;
use crate::object::{ObjClosure, ObjFunction, ObjNativeFunction, ObjString, ObjUpvalue};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ObjRef(usize);

// All heap-allocated object types
pub enum HeapObj {
    String(ObjString),
    Function(ObjFunction),
    NativeFunction(ObjNativeFunction),
    Closure(ObjClosure),
    Upvalue(ObjUpvalue)
}

impl HeapObj {
    pub fn as_string(&self) -> &ObjString {
        match self {
            HeapObj::String(v) => v,
            _ => panic!("Expected String")
        }
    }
    pub fn as_string_mut(&mut self) -> &mut ObjString {
        match self {
            HeapObj::String(v) => v,
            _ => panic!("Expected String")
        }
    }
    pub fn as_function(&self) -> &ObjFunction {
        match self {
            HeapObj::Function(v) => v,
            _ => panic!("Expected Function")
        }
    }
    pub fn as_function_mut(&mut self) -> &mut ObjFunction {
        match self {
            HeapObj::Function(v) => v,
            _ => panic!("Expected Function")
        }
    }
    pub fn as_native_function(&self) -> &ObjNativeFunction {
        match self {
            HeapObj::NativeFunction(v) => v,
            _ => panic!("Expected NativeFunction")
        }
    }
    pub fn as_native_function_mut(&mut self) -> &mut ObjNativeFunction {
        match self {
            HeapObj::NativeFunction(v) => v,
            _ => panic!("Expected NativeFunction")
        }
    }
    pub fn as_closure(&self) -> &ObjClosure {
        match self {
            HeapObj::Closure(v) => v,
            _ => panic!("Expected Closure")
        }
    }
    pub fn as_closure_mut(&mut self) -> &mut ObjClosure {
        match self {
            HeapObj::Closure(v) => v,
            _ => panic!("Expected Closure")
        }
    }
    pub fn as_upvalue(&self) -> &ObjUpvalue {
        match self {
            HeapObj::Upvalue(v) => v,
            _ => panic!("Expected Upvalue")
        }
    }
    pub fn as_upvalue_mut(&mut self) -> &mut ObjUpvalue {
        match self {
            HeapObj::Upvalue(v) => v,
            _ => panic!("Expected Upvalue")
        }
    }
}

/*
* Used to manage Lox runtime objects the VM operates on
 */
pub struct Heap {
    objects: Vec<Option<HeapObj>>,
    free_slots: Vec<usize>, // reusable indices

    // String interning
    strings: HashMap<String, ObjRef>,

    // GC mark phase
    gray_stack: Vec<ObjRef>
}

impl Heap {
    pub fn new() -> Self {
        Heap {
            objects: Vec::new(),
            free_slots: Vec::new(),
            strings: HashMap::new(),
            gray_stack: Vec::new()
        }
    }
    
    pub fn alloc(&mut self, obj: HeapObj) -> ObjRef {
        let index = if let Some(free) = self.free_slots.pop() {
            self.objects[free] = Some(obj);
            free
        } else {
            self.objects.push(Some(obj));
            self.objects.len() - 1
        };
        ObjRef(index)
    }
    
    pub fn get_mut(&mut self, handle: ObjRef) -> &mut HeapObj {
        self.objects[handle.0].as_mut().expect("dangling ObjRef")
    }
    
    pub fn get(&self, handle: ObjRef) -> &HeapObj {
        self.objects[handle.0].as_ref().expect("dangling ObjRef")
    }
    
    pub fn intern_string(&mut self, s: String) -> ObjRef {
        if let Some(&handle) = self.strings.get(&s) {
            return handle;
        }
        let handle = self.alloc(HeapObj::String(ObjString { chars: s.clone() }));
        self.strings.insert(s, handle);
        handle
    }

}
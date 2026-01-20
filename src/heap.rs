use std::collections::HashMap;
use crate::object::{ObjFunction, ObjString};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ObjRef(usize);

// All heap-allocated object types
pub enum Obj {
    String(ObjString),
    Function(ObjFunction)
}

pub struct Heap {
    objects: Vec<Option<Obj>>,
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
    
    pub fn alloc(&mut self, obj: Obj) -> ObjRef {
        let index = if let Some(free) = self.free_slots.pop() {
            self.objects[free] = Some(obj);
            free
        } else {
            self.objects.push(Some(obj));
            self.objects.len() - 1
        };
        ObjRef(index)
    }
    
    pub fn get_mut(&mut self, handle: ObjRef) -> &mut Obj {
        self.objects[handle.0].as_mut().expect("dangling ObjRef")
    }
    
    pub fn get(&self, handle: ObjRef) -> &Obj {
        self.objects[handle.0].as_ref().expect("dangling ObjRef")
    }
    
    pub fn intern_string(&mut self, s: String) -> ObjRef {
        if let Some(&handle) = self.strings.get(&s) {
            return handle;
        }
        let handle = self.alloc(Obj::String(ObjString { chars: s.clone() }));
        self.strings.insert(s, handle);
        handle
    }

}
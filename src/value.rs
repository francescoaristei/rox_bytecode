
/*
Constants values like 1 and 2 in the expression: 1 + 2
need to be stored somewhere in the chunks. Where?
- Directly after the opcode, applies for small integers: called 'immediate instructions'.
- Save bigger data (strings) in specific memory region of the binary executable called 'constants data'.
- Create a pool of constants and store everything there for a chunk (JVM does this with compiled classes) -> clox adopt this solution.
 */
use crate::heap::{Heap, HeapObj, ObjRef};
use crate::value::Value::Obj;
/*
The Lox values in memory are stored in two ways:
 - On the Stack, if the data type is small and immutable.
 - On the Heap, using a pointer to the starting location to access the data for mutable, large data (String, Objects..).
 */
#[derive(Clone, Copy)]
pub enum Value {
    Number(f64),
    Boolean(bool),
    Obj(ObjRef),
    Nil
}

impl Value {
    pub fn print_value(&self, heap: &Heap) {
        match self { 
            Value::Number(number) => print!("{}", number),
            Value::Boolean(bool) => print!("{}", bool),
            Obj(obj_ref) => {
                match heap.get(*obj_ref) {
                    HeapObj::String(string) => print!("{}", string.chars),
                    HeapObj::Function(func) => { self.display_function(func.name, heap) },
                    HeapObj::NativeFunction(native_func) => { self.display_function(native_func.name, heap) },
                    HeapObj::Closure(closure) => { self.display_closure(&closure.obj_function, heap) },
                    HeapObj::Upvalue(_) => print!("upvalue"),
                    HeapObj::Class(class) => { self.display_class(class.name, heap) },
                    HeapObj::Instance(instance) => { self.display_instance(instance.klass, heap) }
                }
            },
            Value::Nil => print!("nil")
        }
    }
    
    fn display_class(&self, obj_ref:Option<ObjRef>, heap: &Heap) {
        match obj_ref { 
            Some(obj_ref) => {
                print!("{}", heap.get(obj_ref).as_string().chars)
            },
            None => {}
        }
    }

    fn display_instance(&self, klass_ref: ObjRef, heap: &Heap) {
        let class = heap.get(klass_ref).as_class();
        if let Some(name_ref) = class.name {
            print!("{} instance", heap.get(name_ref).as_string().chars);
        }
    }

    fn display_closure(&self, obj_ref: &ObjRef, heap: &Heap) {
        self.display_function(heap.get(*obj_ref).as_function().name, heap)
    }

    fn display_function(&self, obj_ref: Option<ObjRef>, heap: &Heap) {
        match obj_ref {
            Some(obj_ref) => {
                print!("<fn {}>", heap.get(obj_ref).as_string().chars)
            },
            None => print!("<script>")
        }
    }

    pub fn is_string(&self, obj_ref: &ObjRef, heap: &Heap) -> bool {
        match heap.get(*obj_ref) {
            HeapObj::String(_) => true,
            _ => false
        }
    }
    
    pub fn as_obj(&self) -> Option<ObjRef> {
        match self { 
            Obj(obj_ref) => Some(*obj_ref),
            _ => None
        }
    }

    pub fn as_string<'a>(&self, heap: &'a Heap) -> Option<&'a str> {
        match self {
            Obj(obj_ref) => {
                match heap.get(*obj_ref) {
                    HeapObj::String(string) => {
                        Some(&string.chars)
                    },
                    _ => None
                }
            },
            _ => None
        }
    }
}

/*impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Number(number) => write!(f, "{}", number),
            Value::Boolean(bool) => write!(f, "{}", bool),
            Value::String(string) => write!(f, "{}", string),
            Value::Nil => write!(f, "nil")
        }
    }
}*/

pub struct Values {
    values: Vec<Value>
}

impl Values {
    pub fn new() -> Self {
        Values {
            values: Vec::new()
        }
    }

    pub fn push_value(&mut self, value: Value) -> usize {
        self.values.push(value);
        self.values.len() - 1
    }

    pub fn get_values(&self) -> &Vec<Value> {
        &self.values
    }

    pub fn get_value(&self, index: usize) -> &Value {
        &self.values[index]
    }
}

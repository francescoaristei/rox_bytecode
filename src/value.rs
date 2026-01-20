
/*
Constants values like 1 and 2 in the expression: 1 + 2
need to be stored somewhere in the chunks. Where?
- Directly after the opcode, applies for small integers: called 'immediate instructions'.
- Save bigger data (strings) in specific memory region of the binary executable called 'constants data'.
- Create a pool of constants and store everything there for a chunk (JVM does this with compiled classes) -> clox adopt this solution.
 */
use std::fmt::{Display, Formatter};
use crate::heap::{Heap, Obj, ObjRef};
/*
The Lox values in memory are stored in two ways:
 - On the Stack, if the data type is small and immutable.
 - On the Heap, using a pointer to the starting location to access the data for mutable, large data (String, Objects..).
 */
#[derive(Clone)]
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
            Value::Obj(obj_ref) => {
                match heap.get(*obj_ref) {
                    Obj::String(string) => print!("{}", string.chars),
                    Obj::Function(func) => {
                        match func.name {
                            Some(obj_ref) => {
                                if let Obj::String(string) = heap.get(obj_ref) {
                                    print!("<fn {}>", string.chars)
                                }
                            },
                            None => print!("<script>")
                        }
                    }
                }
            },
            Value::Nil => print!("nil")
        }
    }

    pub fn is_string(&self, obj_ref: &ObjRef, heap: &Heap) -> bool {
        match heap.get(*obj_ref) {
            Obj::String(_) => true,
            _ => false
        }
    }

    pub fn as_string<'a>(&self, heap: &'a Heap) -> Option<&'a str> {
        match self {
            Value::Obj(obj_ref) => {
                match heap.get(*obj_ref) {
                    Obj::String(string) => {
                        Some(&string.chars)
                    },
                    _ => None
                }
            },
            _ => None
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Number(number) => write!(f, "{}", number),
            Value::Boolean(bool) => write!(f, "{}", bool),
            Value::String(string) => write!(f, "{}", string),
            Value::Nil => write!(f, "nil")
        }
    }
}

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

    pub fn get_value(&self, index: usize) -> &Value {
        &self.values[index]
    }
}


/*
Constants values like 1 and 2 in the expression: 1 + 2
need to be stored somewhere in the chunks. Where?
- Directly after the opcode, applies for small integers: called 'immediate instructions'.
- Save bigger data (strings) in specific memory region of the binary executable called 'constants data'.
- Create a pool of constants and store everything there for a chunk (JVM does this with compiled classes) -> clox adopt this solution.
 */
use std::fmt::{Display, Formatter};
use std::rc::Rc;
/*
The Lox values in memory are stored in two ways:
 - On the Stack, if the data type is small and immutable.
 - On the Heap, using a pointer to the starting location to access the data for mutable, large data (String, Objects..).
 */
#[derive(Clone)]
pub enum Value {
    Number(f64),
    Boolean(bool),
    String(Rc<String>),
    Nil
}

impl Value {
    pub fn print_value(&self) {
        match self { 
            Value::Number(number) => print!("{}", number),
            Value::Boolean(bool) => print!("{}", bool),
            Value::String(string) => print!("{}", string),
            Value::Nil => print!("nil")
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


/*
Constants values like 1 and 2 in the expression: 1 + 2
need to be stored somewhere in the chunks. Where?
- Directly after the opcode, applies for small integers: called 'immediate instructions'.
- Save bigger data (strings) in specific memory region of the binary executable called 'constants data'.
- Create a pool of constants and store everything there for a chunk (JVM does this with compiled classes) -> clox adopt this solution.
 */
use std::fmt::{Display, Formatter};

// TO-DO: later when adding more complex types, use Rc<> only for these complex types, better than using Vec<Rc<Value>> for self.values
// cloning (actual clone) a f64 or a bool is cheap and correct
#[derive(Clone)]
pub enum Value {
    Number(f64)
}

impl Value {
    pub fn print_value(&self) {
        match self { 
            Value::Number(number) => print!("{}", number)
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Number(number) => write!(f, "{}", number)
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

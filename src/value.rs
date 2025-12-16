
/*
Constants values like 1 and 2 in the expression: 1 + 2
need to be stored somewhere in the chunks. Where?
- Directly after the opcode, applies for small integers: called 'immediate instructions'.
- Save bigger data (strings) in specific memory region of the binary executable called 'constants data'.
- Create a pool of constants and store everything there for a chunk (JVM does this with compiled classes) -> clox adopt this solution.
 */
pub type Value = f64;


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

    pub fn print_value(&self, value: &Value) {
        print!("{}", value)
    }
}

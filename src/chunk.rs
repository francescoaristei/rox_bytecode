use crate::value::{Value, Values};

/*
Operation codes for each bytecode instruction.

Stored constants need also to be executed as it could be in: print 1;
The compiled chunk needs therefore to store the constants and know when to execute them.
The bytecode instruction needs not only the opcode, but also operands (binary data stored
after the opcode): in this case the address of where the constant is stored in the constants array.
 */
pub const OP_CONSTANT: u8 = 0;
pub const OP_ADD: u8 = 1;
pub const OP_SUBTRACT: u8 = 2;
pub const OP_MULTIPLY: u8 = 3;
pub const OP_DIVIDE: u8 = 4;
pub const OP_NEGATE: u8 = 5;
pub const OP_RETURN: u8 = 6;

/*
Struct holding the actual bytecode instructions (binary data).
*/
pub struct Chunk {
    // From the AST nodes defined in the interpreter to these three arrays
    code: Vec<u8>,
    lines: Vec<usize>, // to store the line number to return in case of runtime error, it parallels the code array
    constants: Values
}

impl Chunk {
    pub fn new() -> Self {
        Chunk {
            code: Vec::new(),
            lines: Vec::new(),
            constants: Values::new()
        }
    }
    
    pub fn get_code(&self) -> &Vec<u8> {
        &self.code
    }
    
    pub fn get_lines(&self) -> &Vec<usize> {
        &self.lines
    }
    
    pub fn get_constants(&self) -> &Values {
        &self.constants
    }

    pub fn write(&mut self, byte: u8, line: usize) {
        self.code.push(byte);
        self.lines.push(line);
    }

    pub fn read(&self, index: &usize) -> u8 {
        self.code[*index]
    }

    pub fn add_constant(&mut self, value: Value) -> usize {
        self.constants.push_value(value)
    }

    pub fn get_constant(&self, index: usize) -> &Value {
        self.constants.get_value(index)
    }
}
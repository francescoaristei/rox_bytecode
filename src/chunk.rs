use crate::chunk::OpCode::OpCall;
use crate::value::{Value, Values};

/*
Operation codes for each bytecode instruction.

Stored constants need also to be executed as it could be in: print 1;
The compiled chunk needs therefore to store the constants and know when to execute them.
The bytecode instruction needs not only the opcode, but also operands (binary data stored
after the opcode): in this case the address of where the constant is stored in the constants array.
 */
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    OpConstant = 0,
    OpNil = 1,
    OpTrue = 2,
    OpFalse = 3,
    OpPop = 4,
    OpGetGlobal = 5,
    OpGetLocal = 6,
    OpDefineGlobal = 7,
    OpSetGlobal = 8,
    OpSetLocal = 9,
    OpEqual = 10,
    OpGreater = 11,
    OpLess = 12,
    OpAdd = 13,
    OpSubtract = 14,
    OpMultiply = 15,
    OpDivide = 16,
    OpNot = 17,
    OpNegate = 18,
    OpPrint = 19,
    OpJumpIfFalse = 20,
    OpJump = 21,
    OpLoop = 22,
    OpReturn = 23,
    OpCall = 24,
    OpClosure = 25,
    OpGetUpValue = 26,
    OpSetUpValue = 27,
    OpCloseUpValue = 28
}

impl OpCode {
    pub fn from_u8(byte: u8) -> Option<OpCode> {
        match byte {
            0 => Some(OpCode::OpConstant),
            1 => Some(OpCode::OpNil),
            2 => Some(OpCode::OpTrue),
            3 => Some(OpCode::OpFalse),
            4 => Some(OpCode::OpPop),
            5 => Some(OpCode::OpGetGlobal),
            6 => Some(OpCode::OpGetLocal),
            7 => Some(OpCode::OpDefineGlobal),
            8 => Some(OpCode::OpSetGlobal),
            9 => Some(OpCode::OpSetLocal),
            10 => Some(OpCode::OpEqual),
            11 => Some(OpCode::OpGreater),
            12 => Some(OpCode::OpLess),
            13 => Some(OpCode::OpAdd),
            14 => Some(OpCode::OpSubtract),
            15 => Some(OpCode::OpMultiply),
            16 => Some(OpCode::OpDivide),
            17 => Some(OpCode::OpNot),
            18 => Some(OpCode::OpNegate),
            19 => Some(OpCode::OpPrint),
            20 => Some(OpCode::OpJumpIfFalse),
            21 => Some(OpCode::OpJump),
            22 => Some(OpCode::OpLoop),
            23 => Some(OpCode::OpReturn),
            24 => Some(OpCode::OpCall),
            25 => Some(OpCode::OpClosure),
            26 => Some(OpCode::OpGetUpValue),
            27 => Some(OpCode::OpSetUpValue),
            28 => Some(OpCode::OpCloseUpValue),
            _ => None
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

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
    
    pub fn set_code(&mut self, index: usize, value: u8) {
        self.code[index] = value;
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
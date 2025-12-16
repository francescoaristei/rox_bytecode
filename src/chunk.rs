
/*
Operation codes for each bytecode instruction.
 */
use crate::value::{Value, Values};

/*
Stored constants need also to be executed as it could be in: print 1;
The compiled chunk needs therefore to store the constants and know when to execute them.
The bytecode instruction needs not only the opcode, but also operands (binary data stored
after the opcode): in this case the address of where the constant is stored in the constants array.
 */
pub const OP_CONSTANT: u8 = 0;
pub const OP_RETURN: u8 = 1;

/*
Struct holding the actual bytecode instructions (binary data).
*/
pub struct Chunk {
    // From the AST nodes defined in the interpreter to these three arrays
    code: Vec<u8>,
    lines: Vec<u32>, // to store the line number to return in case of runtime error, it parallels the code array
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

    pub fn write(&mut self, byte: u8, line: u32) {
        self.code.push(byte);
        self.lines.push(line);
    }

    pub fn add_constant(&mut self, value: Value) -> usize {
        self.constants.push_value(value)
    }

    pub fn disassemble_chunk(&self, chunk: &Chunk, name: &str) {
        println!("== {} ==", name);
        let mut offset = 0;
        while offset < chunk.code.len() {
            // instructions have different size (number of bytes) therefore
            // the offset is computed when disassembling the instruction itself.
            self.disassemble_instruction(chunk, &mut offset);
        }
    }

    fn disassemble_instruction(&self, chunk: &Chunk, offset: &mut usize) -> usize {
        print!("{:04}", offset);
        // a single line of lox source code, can correspond to several bytecode instructions,
        // '|' is used to mark a bytecode instruction that belongs to the same source code line
        // of the preceding one.
        if *offset > 0 && chunk.lines[*offset] == chunk.lines[*offset - 1] {
            print!("  | ");
        } else {
            print!("{:4}", chunk.lines[*offset]);
        }
        let byte = chunk.code[*offset];
        match byte {
            OP_CONSTANT => self.constant_instruction("OP_CONSTANT", chunk, offset),
            OP_RETURN => self.simple_instruction("OP_RETURN", offset),
            _ => {
                println!("Unknown opcode: {}", byte);
                *offset += 1;
                *offset
            }
        }
    }

    fn constant_instruction(&self, name: &str, chunk: &Chunk, offset: &mut usize) -> usize {
        let constant_index = chunk.code[*offset + 1] as usize;
        print!("    {:16} {:4}  ", name, constant_index);
        chunk.constants.print_value(chunk.constants.get_value(constant_index));
        print!("\n");
        *offset += 2;
        *offset
    }

    fn simple_instruction(&self, name: &str, offset: &mut usize) -> usize {
        print!("    {}", name);
        *offset += 1;
        *offset
    }
}
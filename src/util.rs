use crate::chunk::{Chunk, OP_ADD, OP_CONSTANT, OP_DIVIDE, OP_MULTIPLY, OP_NEGATE, OP_RETURN, OP_SUBTRACT, OP_NIL, OP_TRUE, OP_FALSE, OP_NOT, OP_EQUAL, OP_GREATER, OP_LESS};
use crate::scanner::{Token, TokenType};

// Error handling
pub struct Error {
    report: String,
}

impl Error {
    pub fn error(token: &Token, msg: &str) -> Error {
        let report;
        if token.get_token_type() == TokenType::Eof {
            report = format!("[line {}] Error at end: {}", token.get_line(), msg);
        } else {
            report = format!("[line {}] Error at '{}': {}", token.get_line(), token.get_lexeme(), msg);
        }
        Error {
            report,
        }
    }
}

// Debugging
pub fn disassemble_chunk(chunk: &Chunk, name: &str) {
    println!("== {} ==", name);
    let mut offset = 0;
    while offset < chunk.get_code().len() {
        // instructions have different size (number of bytes) therefore
        // the offset is computed when disassembling the instruction itself.
        offset = disassemble_instruction(chunk, &offset);
    }
}

pub fn disassemble_instruction(chunk: &Chunk, offset: &usize) -> usize {
    print!("{:04}", offset);
    // a single line of lox source code, can correspond to several bytecode instructions,
    // '|' is used to mark a bytecode instruction that belongs to the same source code line
    // of the preceding one.
    if *offset > 0 && chunk.get_lines()[*offset] == chunk.get_lines()[*offset - 1] {
        print!("  | ");
    } else {
        print!("{:4}", chunk.get_lines()[*offset]);
    }
    let byte = chunk.get_code()[*offset];
    match byte {
        OP_CONSTANT => constant_instruction("OP_CONSTANT", chunk, offset),
        OP_TRUE => simple_instruction("OP_TRUE", offset),
        OP_FALSE => simple_instruction("OP_FALSE", offset),
        OP_EQUAL => simple_instruction("OP_EQUAL", offset),
        OP_GREATER => simple_instruction("OP_GREATER", offset),
        OP_LESS => simple_instruction("OP_LESS", offset),
        OP_NIL => simple_instruction("OP_NIL", offset),
        OP_NEGATE => simple_instruction("OP_NEGATE", offset),
        OP_ADD => simple_instruction("OP_ADD", offset),
        OP_SUBTRACT => simple_instruction("OP_SUBTRACT", offset),
        OP_MULTIPLY => simple_instruction("OP_MULTIPLY", offset),
        OP_DIVIDE => simple_instruction("OP_DIVIDE", offset),
        OP_NOT => simple_instruction("OP_NOT", offset),
        OP_RETURN => simple_instruction("OP_RETURN", offset),
        _ => {
            println!("Unknown opcode: {}", byte);
            *offset + 1
        }
    }
}

fn constant_instruction(name: &str, chunk: &Chunk, offset: &usize) -> usize {
    let constant_index = chunk.get_code()[*offset + 1] as usize;
    print!("    {:16} {:4}  ", name, constant_index);
    chunk.get_constants().get_value(constant_index).print_value();
    print!("\n");
    *offset + 2
}

fn simple_instruction(name: &str, offset: &usize) -> usize {
    println!("    {}", name);
    *offset + 1
}
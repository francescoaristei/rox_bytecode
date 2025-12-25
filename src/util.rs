use crate::chunk::{Chunk, OpCode};
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
    match OpCode::from_u8(byte) {
        Some(OpCode::OpConstant) => constant_instruction("OP_CONSTANT", chunk, offset),
        Some(OpCode::OpTrue) => simple_instruction("OP_TRUE", offset),
        Some(OpCode::OpFalse) => simple_instruction("OP_FALSE", offset),
        Some(OpCode::OpEqual) => simple_instruction("OP_EQUAL", offset),
        Some(OpCode::OpGreater) => simple_instruction("OP_GREATER", offset),
        Some(OpCode::OpLess) => simple_instruction("OP_LESS", offset),
        Some(OpCode::OpNil) => simple_instruction("OP_NIL", offset),
        Some(OpCode::OpNegate) => simple_instruction("OP_NEGATE", offset),
        Some(OpCode::OpAdd) => simple_instruction("OP_ADD", offset),
        Some(OpCode::OpSubtract) => simple_instruction("OP_SUBTRACT", offset),
        Some(OpCode::OpMultiply) => simple_instruction("OP_MULTIPLY", offset),
        Some(OpCode::OpDivide) => simple_instruction("OP_DIVIDE", offset),
        Some(OpCode::OpNot) => simple_instruction("OP_NOT", offset),
        Some(OpCode::OpReturn) => simple_instruction("OP_RETURN", offset),
        Some(OpCode::OpPrint) => simple_instruction("OP_PRINT", offset),
        Some(OpCode::OpPop) => simple_instruction("OP_POP", offset),
        Some(OpCode::OpDefineGlobal) => simple_instruction("OP_DEFINE_GLOBAL", offset),
        Some(OpCode::OpGetGlobal) => constant_instruction("OP_GET_GLOBAL", chunk, offset),
        Some(OpCode::OpSetGlobal) => constant_instruction("OP_SET_GLOBAL", chunk, offset),
        None => {
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
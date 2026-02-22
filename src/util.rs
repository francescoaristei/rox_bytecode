use crate::chunk::{Chunk, OpCode};
use crate::heap::Heap;
use crate::scanner::{Token, TokenType};
use crate::value::Value;

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
pub fn disassemble_chunk(chunk: &Chunk, name: &str, heap: &Heap) {
    println!("== {} ==", name);
    let mut offset = 0;
    while offset < chunk.get_code().len() {
        // instructions have different size (number of bytes) therefore
        // the offset is computed when disassembling the instruction itself.
        offset = disassemble_instruction(chunk, &mut offset, heap);
    }
}

pub fn disassemble_instruction(chunk: &Chunk, offset: &mut usize, heap: &Heap) -> usize {
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
        Some(OpCode::OpConstant) => constant_instruction("OP_CONSTANT", chunk, offset, heap),
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
        Some(OpCode::OpGetGlobal) => constant_instruction("OP_GET_GLOBAL", chunk, offset, heap),
        Some(OpCode::OpSetGlobal) => constant_instruction("OP_SET_GLOBAL", chunk, offset, heap),
        Some(OpCode::OpSetLocal) => byte_instruction("OP_SET_LOCAL", chunk, offset),
        Some(OpCode::OpGetLocal) => byte_instruction("OP_GET_LOCAL", chunk, offset),
        Some(OpCode::OpJumpIfFalse) => jump_instruction("OP_JUMP_IF_FALSE", 1, chunk, offset),
        Some(OpCode::OpJump) => jump_instruction("OP_JUMP", 1, chunk, offset),
        Some(OpCode::OpLoop) => jump_instruction("OP_LOOP", -1, chunk, offset),
        Some(OpCode::OpCall) => byte_instruction("OP_CALL", chunk, offset),
        Some(OpCode::OpClosure) => {
            *offset += 1;
            let constant = chunk.get_code()[*offset] as usize;
            *offset += 1;
            print!("    {:16} {:4}", "OP_CLOSURE", constant);
            chunk.get_constants().get_value(constant).print_value(heap);
            println!();
            match chunk.get_constants().get_value(constant) {
                Value::Obj(obj_ref) => {
                    let obj_function = heap.get(*obj_ref).as_function();
                    for i in 0..obj_function.upvalue_count {
                        let is_local = chunk.get_code()[*offset];
                        *offset += 1;
                        let index = chunk.get_code()[*offset];
                        *offset += 1;
                        println!("{:04}      |                     {} {}",
                                 *offset - 2,
                                 if is_local == 1 { "local" } else { "upvalue" },
                                 index
                        );
                    }
                },
                _ => panic!("Must be a obj ref.")
            }
            *offset
        },
        Some(OpCode::OpGetUpValue) => byte_instruction("OP_GET_UPVALUE", chunk, offset),
        Some(OpCode::OpSetUpValue) => byte_instruction("OP_SET_UPVALUE", chunk, offset),
        Some(OpCode::OpCloseUpValue) => simple_instruction("OP_CLOSE_UPVALUE", offset),
        Some(OpCode::OpClass) => constant_instruction("OP_CLASS", chunk, offset, heap),
        Some(OpCode::OpGetProperty) => constant_instruction("OP_GET_PROPERTY", chunk, offset, heap),
        Some(OpCode::OpSetProperty) => constant_instruction("OP_SET_PROPERTY", chunk, offset, heap),
        Some(OpCode::OpMethod) => constant_instruction("OP_METHOD", chunk, offset, heap),
        Some(OpCode::OpInvoke) => invoke_instruction("OP_INVOKE", chunk, offset, heap),
        Some(OpCode::OpInherit) => simple_instruction("OP_INHERIT", offset),
        Some(OpCode::OpGetSuper) => constant_instruction("OP_GET_SUPER", chunk, offset, heap),
        Some(OpCode::OpSuperInvoke) => invoke_instruction("OP_SUPER_INVOKE", chunk, offset, heap),
        None => {
            println!("Unknown opcode: {}", byte);
            *offset + 1
        }
    }
}

fn jump_instruction(name: &str, sign: i32, chunk: &Chunk, offset: &usize) -> usize {
    let mut jump = (chunk.get_code()[offset + 1] as u16) << 8;
    jump |= chunk.get_code()[offset + 2] as u16;
    let target = (*offset as i32) + 3 + sign * (jump as i32);
    print!("    {:16} {:4} -> {}", name, offset, target);
    println!();
    *offset + 3
}

fn byte_instruction(name: &str, chunk: &Chunk, offset: &usize) -> usize {
    // local variables never leave the compiler to make it to the chunk therefore we show only the slot
    let slot = chunk.get_code()[offset + 1];
    print!("    {:16} {:4}  ", name, slot);
    println!();
    *offset + 2
}

fn constant_instruction(name: &str, chunk: &Chunk, offset: &usize, heap: &Heap) -> usize {
    let constant_index = chunk.get_code()[*offset + 1] as usize;
    print!("    {:16} {:4}  ", name, constant_index);
    chunk.get_constants().get_value(constant_index).print_value(heap);
    println!();
    *offset + 2
}

fn invoke_instruction(name: &str, chunk: &Chunk, offset: &usize, heap: &Heap) -> usize {
    let constant_index = chunk.get_code()[*offset + 1] as usize;
    let arg_count = chunk.get_code()[*offset + 2];
    print!("    {:16} ({} args) {:4}  ", name, arg_count, constant_index);
    chunk.get_constants().get_value(constant_index).print_value(heap);
    println!();
    *offset + 3
}

fn simple_instruction(name: &str, offset: &usize) -> usize {
    println!("    {}", name);
    *offset + 1
}
use rox_bytecode::chunk::{Chunk, OP_CONSTANT, OP_RETURN};

fn main() {
    let mut chunk = Chunk::new();
    let constant = chunk.add_constant(1.2);
    chunk.write(OP_CONSTANT, 123);
    chunk.write(constant as u8, 123);
    chunk.write(OP_RETURN, 123);
    chunk.disassemble_chunk(&chunk, "test chunk");
}

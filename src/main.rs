use rox_bytecode::chunk::{Chunk, OP_CONSTANT, OP_RETURN};
use rox_bytecode::value::Value;
use rox_bytecode::vm::Vm;

fn main() {
    let mut chunk = Chunk::new();
    let constant = chunk.add_constant(Value::Number(1.2));
    chunk.write(OP_CONSTANT, 123);
    chunk.write(constant as u8, 123);
    chunk.write(OP_RETURN, 123);
    chunk.disassemble_chunk(&chunk, "test chunk");
    let mut vm = Vm::new(chunk);
    vm.run();
}

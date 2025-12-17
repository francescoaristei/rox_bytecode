use rox_bytecode::chunk::{Chunk, OP_ADD, OP_CONSTANT, OP_DIVIDE, OP_NEGATE, OP_RETURN};
use rox_bytecode::value::Value;
use rox_bytecode::vm::Vm;

fn main() {
    let mut chunk = Chunk::new();
    let constant_1 = chunk.add_constant(Value::Number(1.2));
    chunk.write(OP_CONSTANT, 123);
    chunk.write(constant_1 as u8, 123);

    let constant_2 = chunk.add_constant(Value::Number(3.4));
    chunk.write(OP_CONSTANT, 123);
    chunk.write(constant_2 as u8, 123);

    chunk.write(OP_ADD, 123);

    let constant_3 = chunk.add_constant(Value::Number(5.6));
    chunk.write(OP_CONSTANT, 123);
    chunk.write(constant_3 as u8, 123);

    chunk.write(OP_DIVIDE, 123);

    chunk.write(OP_NEGATE, 123);
    chunk.write(OP_RETURN, 123);
    chunk.disassemble_chunk(&chunk, "test chunk");
    let mut vm = Vm::new(chunk);
    vm.run();
}

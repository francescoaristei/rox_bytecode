use crate::chunk::Chunk;
use crate::chunk::OP_RETURN;
use crate::chunk::OP_CONSTANT;
use crate::value::Value;

pub struct Vm {
    chunk: Chunk,
    ip: usize
}

pub enum InterpretResult {
    InterpretOk,
    InterpretCompileError,
    InterpretRuntimeError
}

impl Vm {
    pub fn new(chunk: Chunk) -> Self {
        Vm {
            chunk,
            ip: 0
        }
    }

    pub fn run(&mut self) -> InterpretResult {
        loop {

            #[cfg(feature = "debug-trace-execution")]
            {
                // use: cargo run --features debug-trace-execution
                self.chunk.disassemble_instruction(&self.chunk, &mut self.ip);
            }

            // opcode of the bytecode instruction is read to understand what to do, process called 'dispatching'
            let instruction: u8 = self.read_byte();
            match instruction {
                OP_RETURN => return InterpretResult::InterpretOk,
                OP_CONSTANT => {
                    let constant = self.read_constant();
                    constant.print_value();
                    print!("\n");
                }
                _ => ()
            }
        }
    }

    fn read_byte(&mut self) -> u8 {
        let bytecode = self.chunk.read(&self.ip);
        self.ip += 1;
        bytecode
    }

    fn read_constant(&mut self) -> &Value {
        let index = self.read_byte() as usize;
        self.chunk.get_constant(index)
    }
}


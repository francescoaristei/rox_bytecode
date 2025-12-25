use std::{env, fs, io, process};
use std::io::Write;
use rox_bytecode::chunk::Chunk;
use rox_bytecode::vm::{Error, Vm};

fn main() {
    let args: Vec<String> = env::args().collect();
    let file_name = "C:\\Users\\Utente\\RustroverProjects\\rox_bytecode\\src\\temp.lox";
    run_file(file_name);
    /*if args.len() > 2 {
        println!("Usage: Rox [script]");
        process::exit(0);
    } else if args.len() == 2 {
        run_file(args[1].as_str());
    } else {
        run_prompt();
    }*/
}

fn run_file(path: &str) {
    match read_file(path) {
        Ok(source) => {
            match run(source) {
                Ok(()) => (),
                Err(error) => {
                    match error {
                        Error::InterpretCompilerError => process::exit(65),
                        Error::InterpretRuntimeError => process::exit(70),
                    }
                }
            }
        },
        Err(_) => eprint!("Error reading the file.")
    }
}

fn run_prompt() {
    loop {
        print!("> ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            // CTRL + Z
            Ok(0) => break,
            Ok(_) => {
                let input = input.trim().to_string();
                match run(input) {
                    Ok(()) => (),
                    Err(_) => break
                }
            },
            Err(_) => {
                println!("Error reading line.");
                break;
            }
        }
    }
}

fn run(source: String) -> Result<(), Error> {
    let chunk = Chunk::new();
    let mut vm = Vm::new(chunk);
    vm.interpret(source)
}

fn read_file(path: &str) -> io::Result<String> {
    fs::read_to_string(path)
}

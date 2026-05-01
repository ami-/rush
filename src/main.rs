#[allow(unused_imports)]
use std::io::{self, Write};

fn main() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut command = String::new();
        io::stdin()
            .read_line(&mut command)
            .expect("reading from io");

        let cmd = command.trim();

        match cmd {
            "exit" => break,
            cmd => {
                println!("{}: command not found", cmd);
            }
        }
    }
}

#[allow(unused_imports)]
use std::io::{self, Write};

fn main() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        io::stdin().read_line(&mut line).expect("reading from io");

        let line = line.trim();

        match line {
            l if line.starts_with("echo ") || line == "echo" => {
                let start = "echo".len();
                println!("{}", &l[start..].trim());
            }
            "exit" => break,
            cmd => {
                println!("{}: command not found", cmd);
            }
        }
    }
}

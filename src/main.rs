#[allow(unused_imports)]
use std::io::{self, Write};

const BUILTINS: [&str; 3] = ["echo", "exit", "type"];

fn main() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        io::stdin().read_line(&mut line).expect("reading from io");

        let (cmd, args) = parse_command(line.trim());

        match cmd {
            "type" => {
                let (cmd, _) = parse_command(args);
                if BUILTINS.contains(&cmd) {
                    println!("{} is a shell builtin", cmd);
                } else {
                    println!("{}: not found", cmd);
                }
            }
            "echo" => println!("{}", args),
            "exit" => break,
            _ => println!("{}: command not found", cmd),
        }
    }
}

fn parse_command(line: &str) -> (&str, &str) {
    let first_word = line.split_whitespace().next().unwrap_or("");
    let rest = line[first_word.len()..].trim();
    (first_word, rest)
}

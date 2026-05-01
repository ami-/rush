use std::env;
#[allow(unused_imports)]
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;

const BUILTINS: [&str; 3] = ["echo", "exit", "type"];

fn main() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        io::stdin().read_line(&mut line).expect("reading from io");

        let (cmd, args) = parse_command(line.trim());

        match cmd {
            "type" => do_type(args),
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

fn do_type(line: &str) {
    let (cmd, _) = parse_command(line);
    if BUILTINS.contains(&cmd) {
        println!("{} is a shell builtin", cmd);
        return;
    }
    if let Ok(path) = env::var("PATH") {
        for dir in env::split_paths(&path) {
            let full_path = dir.join(cmd);
            if let Ok(meta) = full_path.metadata() {
                if meta.permissions().mode() & 0o111 != 0 {
                    println!("{} is {}", cmd, full_path.display());
                    return;
                }
            }
        }
    }
    println!("{}: not found", cmd);
}

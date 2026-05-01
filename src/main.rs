#[allow(unused_imports)]
use std::io::{self, Write};

fn main() {
    print!("$ ");
    io::stdout().flush().unwrap();

    let mut command = String::new();
    io::stdin()
        .read_line(&mut command)
        .expect("reading from io");
    print!("{}: command not found", command.trim());
}

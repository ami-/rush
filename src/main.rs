use std::env::{self, set_current_dir};
#[allow(unused_imports)]
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{self, Path};
use std::process::Command;

const BUILTINS: &[&str] = &["echo", "exit", "type", "pwd", "cd"];

fn main() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        io::stdin().read_line(&mut line).expect("reading from io");

        let (cmd, args) = parse_command(line.trim());

        match cmd {
            "exit" => break,
            "echo" => println!("{}", args),
            "type" => do_type(args),
            "pwd" => do_pwd(),
            "cd" => do_cd(args),
            _ if let Some(exe_path) = find_executable(cmd) => {
                let arg_i = args.split_whitespace();
                let exe = exe_path.file_name().expect("bad file name");
                let _ = Command::new(exe).args(arg_i).status();
            }
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
    match find_executable(cmd) {
        Some(full_path) => println!("{} is {}", cmd, full_path.display()),
        None => println!("{}: not found", cmd),
    };
}

fn find_executable(name: &str) -> Option<path::PathBuf> {
    if let Ok(path) = env::var("PATH") {
        for dir in env::split_paths(&path) {
            let full_path = dir.join(name);
            if full_path.is_file()
                && let Ok(meta) = full_path.metadata()
            {
                if meta.permissions().mode() & 0o111 != 0 {
                    return Some(full_path);
                }
            }
        }
    }
    None
}

fn do_pwd() {
    if let Ok(dir) = env::current_dir() {
        println!("{}", dir.display())
    }
}

fn do_cd(path: &str) {
    if path.starts_with("/") {
        let dir = Path::new(path);
        if dir.exists() {
            set_current_dir(dir).expect("change directory");
        } else {
            println!("cd: {}: No such file or directory", path);
        }
    }
}

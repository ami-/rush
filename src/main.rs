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

        let tokens = parse_cmd(line.trim());
        let args: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();

        match args.as_slice() {
            [] => continue,
            ["exit", ..] => break,
            ["echo", args @ ..] => println!("{}", args.join(" ")),
            ["type", args @ ..] => do_type(args),
            ["pwd"] => do_pwd(),
            ["cd", args @ ..] => do_cd(args),
            _ if let Some(exe_path) = find_executable(args[0]) => {
                let exe = exe_path.file_name().expect("bad file name");
                let _ = Command::new(exe).args(args[1..].iter()).status();
            }
            [cmd, ..] => println!("{}: command not found", cmd),
        }
    }
}

fn do_type(args: &[&str]) {
    if args.len() == 0 {
        println!("type: needs argument");
        return;
    }
    let cmd = args[0];
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

fn do_cd(args: &[&str]) {
    if args.len() == 0 {
        println!("cd: needs argument");
        return;
    }
    let path = args[0].replace("~", env::var("HOME").unwrap().as_str());
    let dir = Path::new(&path);
    if dir.exists() {
        set_current_dir(dir).expect("change directory");
    } else {
        println!("cd: {}: No such file or directory", path.to_string());
    }
}

fn parse_cmd(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut in_sq = false;
    let mut in_dq = false;

    const SQ: char = '\'';
    const DQ: char = '"';

    let mut buf = String::new();

    let mut cn: char;
    let mut it = line.chars().peekable();
    while let Some(cc) = it.next() {
        cn = ' ';
        if let Some(&c) = it.peek() {
            cn = c
        }
        if !in_dq {
            if cc == SQ && cn == SQ {
                let _ = it.next();
                continue;
            }
            if cc == SQ && cn != SQ {
                in_sq = !in_sq;
                continue;
            }
        }
        if !in_sq {
            if cc == DQ && cn == DQ {
                let _ = it.next();
                continue;
            }
            if cc == DQ && cn != DQ {
                in_dq = !in_dq;
                continue;
            }
        }
        if !in_sq && !in_dq && cc.is_ascii_whitespace() {
            if buf.len() > 0 {
                out.push(buf.clone());
                buf.clear();
            }
            continue;
        }
        buf.push(cc);
    }
    if buf.len() > 0 {
        out.push(buf)
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_squotes() {
        let result = parse_cmd("hello");
        assert_eq!(result[0], "hello");
    }
    #[test]
    fn empty_squotes() {
        let result = parse_cmd("''hello");
        assert_eq!(result[0], "hello");
    }
    #[test]
    fn simple_squotes() {
        let result = parse_cmd("'hello'    'bau'");
        assert_eq!(result[0], "hello");
        assert_eq!(result[1], "bau");
    }
    #[test]
    fn concat_squotes() {
        let result = parse_cmd("first 'hello''bau' 'cucu'");
        assert_eq!(result[0], "first");
        assert_eq!(result[1], "hellobau");
        assert_eq!(result[2], "cucu");
    }
    #[test]
    fn multi_squotes_inside() {
        let result = parse_cmd("'hello''''bau'");
        assert_eq!(result[0], "hellobau");
    }
    #[test]
    fn multi_squotes_outside() {
        let result = parse_cmd("hello''''bau");
        assert_eq!(result[0], "hellobau");
    }

    #[test]
    fn multi_squotes2() {
        let result = parse_cmd("echo 'shell     test' 'hello''example' script''world");
        assert_eq!(result[0], "echo");
        assert_eq!(result[1], "shell     test");
        assert_eq!(result[2], "helloexample");
        assert_eq!(result[3], "scriptworld");
    }
    #[test]
    fn preserve_space() {
        let result = parse_cmd("'hello    world'");
        assert_eq!(result[0], "hello    world");
    }
    #[test]
    fn simple_dquote() {
        let result = parse_cmd(r#""hello    world""#);
        assert_eq!(result[0], "hello    world");
    }
    #[test]
    fn multi1_dquoe() {
        let result = parse_cmd(r#""hello" 'bau'   "world""#);
        assert_eq!(result[0], "hello");
        assert_eq!(result[1], "bau");
        assert_eq!(result[2], "world");
    }
    #[test]
    fn multi2_dquoe() {
        let result = parse_cmd(r#""hello""world""#);
        assert_eq!(result[0], "helloworld");
    }
    #[test]
    fn combined_dquoe() {
        let result = parse_cmd(r#""hell's kitchen""#);
        assert_eq!(result[0], "hell's kitchen");
    }
}

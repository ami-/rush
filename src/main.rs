mod parse;
mod readline;
mod redirect;

use std::env::{self, set_current_dir};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{self, Path, PathBuf};
use std::process::Command;

use rustyline::error::ReadlineError;

use parse::parse_cmd;
use redirect::{Redirects, split_redirect};

pub const BUILTINS: &[&str] = &["echo", "exit", "type", "pwd", "cd"];

fn main() {
    let mut rl = readline::create_editor().expect("create line editor");

    loop {
        let line = match rl.readline("$ ") {
            Ok(l) => l,
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("{}", e);
                break;
            }
        };

        let tokens = parse_cmd(line.trim());
        let all_args: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();
        if all_args.is_empty() {
            continue;
        }
        let (tail, redir) = split_redirect(&all_args[1..]);
        let mut args = vec![all_args[0]];
        args.extend_from_slice(&tail);

        match args.as_slice() {
            [] => continue,
            ["exit", ..] => break,
            ["echo", args @ ..] => {
                let _ = redir.open_stderr_write();
                if let Ok(mut out) = redir.open_stdout_write() {
                    let _ = writeln!(out, "{}", args.join(" "));
                }
            }
            ["type", args @ ..] => {
                let mut out = redir
                    .open_stdout_write()
                    .unwrap_or_else(|_| Box::new(io::stdout()));
                let mut err = redir
                    .open_stderr_write()
                    .unwrap_or_else(|_| Box::new(io::stderr()));
                let _ = do_type(args, &mut *out, &mut *err);
            }
            ["pwd"] => {
                let mut out = redir
                    .open_stdout_write()
                    .unwrap_or_else(|_| Box::new(io::stdout()));
                let mut err = redir
                    .open_stderr_write()
                    .unwrap_or_else(|_| Box::new(io::stderr()));
                let _ = do_pwd(&mut *out, &mut *err);
            }
            ["cd", args @ ..] => {
                let mut err = redir
                    .open_stderr_write()
                    .unwrap_or_else(|_| Box::new(io::stderr()));
                do_cd(args, &mut *err);
            }
            _ if let Some(exe_path) = find_executable(args[0]) => {
                do_cmd(exe_path, &tail, redir);
            }
            [cmd, ..] => eprintln!("{}: command not found", cmd),
        }
    }
}

fn do_type(args: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> io::Result<()> {
    if args.len() == 0 {
        return writeln!(err, "type: needs argument");
    }
    let cmd = args[0];
    if BUILTINS.contains(&cmd) {
        return writeln!(out, "{} is a shell builtin", cmd);
    }
    match find_executable(cmd) {
        Some(full_path) => writeln!(out, "{} is {}", cmd, full_path.display()),
        None => writeln!(err, "{}: not found", cmd),
    }
}

fn do_pwd(out: &mut dyn Write, _err: &mut dyn Write) -> io::Result<()> {
    let dir = env::current_dir()?;
    writeln!(out, "{}", dir.display())
}

fn do_cd(args: &[&str], err: &mut dyn Write) {
    if args.len() == 0 {
        let _ = writeln!(err, "cd: needs argument");
        return;
    }
    let path = args[0].replace("~", env::var("HOME").unwrap().as_str());
    let dir = Path::new(&path);
    if dir.exists() {
        set_current_dir(dir).expect("change directory");
    } else {
        let _ = writeln!(err, "cd: {}: No such file or directory", path);
    }
}

pub fn executables_with_prefix(prefix: &str) -> Vec<String> {
    use std::collections::HashSet;
    let Ok(path) = env::var("PATH") else {
        return vec![];
    };
    let mut names: HashSet<String> = HashSet::new();
    for dir in env::split_paths(&path) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if meta.is_file() && meta.permissions().mode() & 0o111 != 0 && name.starts_with(prefix)
            {
                names.insert(name);
            }
        }
    }
    let mut result: Vec<String> = names.into_iter().collect();
    result.sort();
    result
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

fn do_cmd(exe_path: PathBuf, args: &[&str], redir: Redirects) {
    match redir.open_stdio() {
        Ok((stdout, stderr)) => {
            let exe = exe_path.file_name().expect("bad exe path");
            let _ = Command::new(exe)
                .args(args)
                .stdout(stdout)
                .stderr(stderr)
                .status();
        }
        Err(e) => eprintln!("{}", e),
    }
}

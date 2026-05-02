mod parse;
mod readline;
mod redirect;

use std::cell::RefCell;
use std::collections::HashMap;
use std::env::{self, set_current_dir};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{self, Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use rustyline::error::ReadlineError;

use parse::parse_cmd;
use redirect::{Redirects, split_redirect};

pub const BUILTINS: &[&str] = &["echo", "exit", "type", "pwd", "cd", "complete"];

fn main() {
    let completions: Rc<RefCell<HashMap<String, String>>> = Rc::new(RefCell::new(HashMap::new()));
    let mut rl = readline::create_editor(Rc::clone(&completions)).expect("create line editor");

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
                let _ = do_cd(args, &mut *err);
            }
            ["complete", args @ ..] => {
                let mut out = redir
                    .open_stdout_write()
                    .unwrap_or_else(|_| Box::new(io::stdout()));
                let mut err = redir
                    .open_stderr_write()
                    .unwrap_or_else(|_| Box::new(io::stderr()));
                let _ = do_complete(args, &mut *out, &mut *err, &completions);
            }
            _ if let Some(exe_path) = find_executable(args[0]) => {
                let _ = do_cmd(exe_path, &tail, redir);
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

fn do_cd(args: &[&str], err: &mut dyn Write) -> io::Result<()> {
    if args.len() == 0 {
        return writeln!(err, "cd: needs argument");
    }
    let path = args[0].replace("~", env::var("HOME").unwrap().as_str());
    let dir = Path::new(&path);
    if dir.exists() {
        set_current_dir(dir)
    } else {
        writeln!(err, "cd: {}: No such file or directory", path)
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

fn do_cmd(exe_path: PathBuf, args: &[&str], redir: Redirects) -> io::Result<()> {
    let (stdout, stderr) = redir.open_stdio()?;
    let exe = exe_path.file_name().expect("bad exe path");
    Command::new(exe)
        .args(args)
        .stdout(stdout)
        .stderr(stderr)
        .status()?;
    Ok(())
}

fn do_complete(
    args: &[&str],
    out: &mut dyn Write,
    err: &mut dyn Write,
    registry: &RefCell<HashMap<String, String>>,
) -> io::Result<()> {
    let mut idx = 0;
    while idx < args.len() {
        match args[idx] {
            "-p" => {
                let Some(name) = args.get(idx + 1).copied() else {
                    idx += 1;
                    continue;
                };
                if let Some(cmd) = registry.borrow().get(name) {
                    writeln!(out, "complete -C '{}' {}", cmd, name)?;
                } else {
                    writeln!(err, "complete: {}: no completion specification", name)?;
                }
                idx += 2;
            }
            "-C" => {
                let Some(cmd) = args.get(idx + 1).copied() else {
                    idx += 1;
                    continue;
                };
                let Some(name) = args.get(idx + 2).copied() else {
                    idx += 2;
                    continue;
                };
                registry
                    .borrow_mut()
                    .insert(name.to_owned(), cmd.to_owned());
                idx += 3;
            }
            "-r" => {
                let Some(cmd) = args.get(idx + 1).copied() else {
                    idx += 1;
                    continue;
                };
                registry.borrow_mut().remove(cmd);
                idx += 2;
            }
            _ => idx += 1,
        }
    }
    Ok(())
}

mod parse;
mod readline;
mod redirect;

use std::cell::RefCell;
use std::collections::HashMap;
use std::env::{self, set_current_dir};
use std::io::{self, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{self, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;

use rustyline::error::ReadlineError;

use parse::parse_cmd;
use parse::split_pipeline;
use redirect::{Redirects, split_redirect};

pub const BUILTINS: &[&str] = &[
    "echo", "exit", "type", "pwd", "cd", "complete", "jobs", "history",
];

#[derive(Debug)]
struct JobDescriptor {
    number: u32,
    pid: u32,
    cmd: String,
    child: Child,
}

fn main() {
    let completions: Rc<RefCell<HashMap<String, String>>> = Rc::new(RefCell::new(HashMap::new()));
    let mut rl = readline::create_editor(Rc::clone(&completions)).expect("create line editor");
    let mut job_data: Vec<JobDescriptor> = Vec::new();

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
        let segments = split_pipeline(tokens);

        if segments.len() == 1 {
            let all_args: Vec<&str> = segments[0].iter().map(|s| s.as_str()).collect();
            if all_args.is_empty() {
                continue;
            }
            let (tail, redir) = split_redirect(&all_args[1..]);
            let mut args = vec![all_args[0]];
            args.extend_from_slice(&tail);

            let mut out = redir
                .open_stdout_write()
                .unwrap_or_else(|_| Box::new(io::stdout()));
            let mut err = redir
                .open_stderr_write()
                .unwrap_or_else(|_| Box::new(io::stderr()));

            let result: io::Result<()> = match args.as_slice() {
                [] => continue,
                ["exit", ..] => break,
                [cmd, rest @ .., "&"] => do_spawn(cmd, rest, &mut *out, &mut *err, &mut job_data),
                _ if BUILTINS.contains(&args[0]) => run_builtin(
                    args[0],
                    &tail,
                    &mut *out,
                    &mut *err,
                    &completions,
                    &mut job_data,
                ),
                _ if let Some(exe_path) = find_executable(args[0]) => {
                    do_cmd(exe_path, &tail, redir)
                }
                [cmd, ..] => Err(io::Error::other(format!("{}: command not found", cmd))),
            };
            if let Err(e) = result {
                let _ = writeln!(err, "{}", e);
            }
        } else {
            let _ = do_pipeline(segments, &completions, &mut job_data);
        }

        let mut jobs_out = Box::new(io::stdout());
        let mut jobs_err = Box::new(io::stderr());
        let _ = do_jobs(&mut *jobs_out, &mut *jobs_err, &mut job_data, true);
    }
}

fn do_echo(args: &[&str], out: &mut dyn Write) -> io::Result<()> {
    writeln!(out, "{}", args.join(" "))
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
fn do_jobs(
    out: &mut dyn Write,
    err: &mut dyn Write,
    jobs: &mut Vec<JobDescriptor>,
    print_done_only: bool,
) -> io::Result<()> {
    //TODO: fg bg influence last
    let last = jobs.iter().map(|jd| jd.number).max().unwrap_or(0);
    let prev = jobs
        .iter()
        .filter(|jd| jd.number != last)
        .map(|jd| jd.number)
        .max();
    let mut to_remove = vec![];
    for jd in jobs.iter_mut() {
        let marker = match jd.number {
            n if n == last => "+",
            n if let Some(p) = prev
                && n == p =>
            {
                "-"
            }
            _ => " ",
        };
        let status = match jd.child.try_wait() {
            Ok(Some(_)) => "Done",
            Ok(None) => "Running",
            Err(e) => {
                writeln!(err, "jobs: {}: {}", jd.pid, e)?;
                continue;
            }
        };
        if status == "Done" {
            to_remove.push(jd.number);
        }
        if print_done_only && status != "Done" {
            continue;
        } else {
            writeln!(out, "[{}]{}  {: <24}{}", jd.number, marker, status, jd.cmd,)?;
        }
    }
    jobs.retain(|jd| !to_remove.contains(&jd.number));

    Ok(())
}
fn do_spawn(
    cmd: &str,
    args: &[&str],
    out: &mut dyn Write,
    _err: &mut dyn Write,
    jobs: &mut Vec<JobDescriptor>,
) -> io::Result<()> {
    let child = Command::new(cmd).args(args).spawn()?;
    let pid = child.id();
    let number = jobs.iter().map(|jd| jd.number).max().unwrap_or(0) + 1;
    let cmd = [cmd]
        .iter()
        .chain(args.iter())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");

    jobs.push(JobDescriptor {
        number,
        pid,
        cmd,
        child,
    });

    writeln!(out, "[{number}] {pid}")?;

    Ok(())
}

fn run_builtin(
    name: &str,
    args: &[&str],
    out: &mut dyn Write,
    err: &mut dyn Write,
    completions: &RefCell<HashMap<String, String>>,
    jobs: &mut Vec<JobDescriptor>,
) -> io::Result<()> {
    match name {
        "echo" => do_echo(args, out),
        "type" => do_type(args, out, err),
        "pwd" => do_pwd(out, err),
        "cd" => do_cd(args, err),
        "complete" => do_complete(args, out, err, completions),
        "jobs" => do_jobs(out, err, jobs, false),
        "history" => do_history(out, err),
        "exit" => Ok(()), // in a pipeline exit only closes this segment's pipe, not the shell
        _ => Ok(()),
    }
}

fn do_pipeline(
    segments: Vec<Vec<String>>,
    completions: &RefCell<HashMap<String, String>>,
    jobs: &mut Vec<JobDescriptor>,
) -> io::Result<()> {
    let n = segments.len();
    let mut children: Vec<std::process::Child> = Vec::new();
    let mut prev_read: Option<OwnedFd> = None;

    for (i, seg) in segments.iter().enumerate() {
        let is_last = i == n - 1;

        let all_args: Vec<&str> = seg.iter().map(|s| s.as_str()).collect();
        if all_args.is_empty() {
            continue;
        }

        let cmd_name = all_args[0];
        let (args, redir) = split_redirect(&all_args[1..]);

        if BUILTINS.contains(&cmd_name) {
            if is_last {
                let mut out = redir
                    .open_stdout_write()
                    .unwrap_or_else(|_| Box::new(io::stdout()));
                let mut err = redir
                    .open_stderr_write()
                    .unwrap_or_else(|_| Box::new(io::stderr()));
                run_builtin(cmd_name, &args, &mut *out, &mut *err, completions, jobs)?;
            } else {
                let (pipe_read, pipe_write) = std::io::pipe()?;
                let mut out: Box<dyn Write> = Box::new(pipe_write);
                let mut err_w: Box<dyn Write> = Box::new(io::stderr());
                run_builtin(cmd_name, &args, &mut *out, &mut *err_w, completions, jobs)?;
                drop(out); // close write end → next command sees EOF
                prev_read = Some(pipe_read.into());
            }
        } else {
            let stdin_stdio = match prev_read.take() {
                Some(fd) => Stdio::from(fd),
                None => Stdio::inherit(),
            };

            let (redir_stdout, redir_stderr) = redir.open_stdio()?;
            let stdout_stdio = if is_last {
                redir_stdout
            } else {
                Stdio::piped()
            };

            let exe = find_executable(cmd_name)
                .ok_or_else(|| io::Error::other(format!("{cmd_name}: command not found")))?;

            let mut child = Command::new(exe)
                .args(&args)
                .stdin(stdin_stdio)
                .stdout(stdout_stdio)
                .stderr(redir_stderr)
                .spawn()?;

            if !is_last {
                let fd: OwnedFd = child.stdout.take().unwrap().into();
                prev_read = Some(fd);
            }
            children.push(child);
        }
    }

    // Wait for all children AFTER all are spawned
    // (spawning all first prevents deadlock when pipe buffers fill)
    for mut child in children {
        child.wait()?;
    }
    Ok(())
}
fn do_history(_out: &mut dyn Write, _err: &mut dyn Write) -> io::Result<()> {
    Ok(())
}

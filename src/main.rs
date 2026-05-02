use std::env::{self, set_current_dir};
use std::fs::{File, OpenOptions};
#[allow(unused_imports)]
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{self, Path, PathBuf};
use std::process::{Command, Stdio};

const BUILTINS: &[&str] = &["echo", "exit", "type", "pwd", "cd"];

#[derive(Debug)]
enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug)]
enum OutDest {
    Inherit,
    File { append: bool, path: String },
    SameAs(Stream),
}

impl OutDest {
    fn open_file(&self) -> Result<Option<File>, io::Error> {
        match self {
            OutDest::Inherit | OutDest::SameAs(_) => Ok(None),
            OutDest::File { append, path } => {
                if let Some(parent) = Path::new(path).parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent)?;
                    }
                }
                let f: File = if *append {
                    OpenOptions::new().append(true).create(true).open(path)?
                } else {
                    File::create(path)?
                };
                Ok(Some(f))
            }
        }
    }
}

#[derive(Debug)]
struct Redirects {
    stdout: OutDest,
    stderr: OutDest,
}

impl Redirects {
    fn none() -> Self {
        Redirects {
            stdout: OutDest::Inherit,
            stderr: OutDest::Inherit,
        }
    }

    fn open_stdio(&self) -> io::Result<(Stdio, Stdio)> {
        match (&self.stdout, &self.stderr) {
            // cycle: both inherit
            (OutDest::SameAs(Stream::Stderr), OutDest::SameAs(Stream::Stdout)) => {
                Ok((Stdio::inherit(), Stdio::inherit()))
            }
            // stdout -> stderr's destination
            (OutDest::SameAs(Stream::Stderr), _) => {
                let err_file = self.stderr.open_file()?;
                let out = match &err_file {
                    Some(f) => Stdio::from(f.try_clone()?),
                    None => Stdio::inherit(),
                };
                let err = match err_file {
                    Some(f) => Stdio::from(f),
                    None => Stdio::inherit(),
                };
                Ok((out, err))
            }
            // stderr -> stdout's destination
            (_, OutDest::SameAs(Stream::Stdout)) => {
                let out_file = self.stdout.open_file()?;
                let err = match &out_file {
                    Some(f) => Stdio::from(f.try_clone()?),
                    None => Stdio::inherit(),
                };
                let out = match out_file {
                    Some(f) => Stdio::from(f),
                    None => Stdio::inherit(),
                };
                Ok((out, err))
            }
            // independent destinations
            _ => {
                let out = match self.stdout.open_file()? {
                    Some(f) => Stdio::from(f),
                    None => Stdio::inherit(),
                };
                let err = match self.stderr.open_file()? {
                    Some(f) => Stdio::from(f),
                    None => Stdio::inherit(),
                };
                Ok((out, err))
            }
        }
    }

    fn open_stdout_write(&self) -> io::Result<Box<dyn Write>> {
        match &self.stdout {
            OutDest::SameAs(Stream::Stderr) => Ok(Box::new(io::stderr())),
            _ => match self.stdout.open_file()? {
                Some(f) => Ok(Box::new(f)),
                None => Ok(Box::new(io::stdout())),
            },
        }
    }

    fn open_stderr_write(&self) -> io::Result<Box<dyn Write>> {
        match &self.stderr {
            OutDest::SameAs(Stream::Stdout) => match self.stdout.open_file()? {
                Some(f) => Ok(Box::new(f)),
                None => Ok(Box::new(io::stdout())),
            },
            _ => match self.stderr.open_file()? {
                Some(f) => Ok(Box::new(f)),
                None => Ok(Box::new(io::stderr())),
            },
        }
    }
}

fn main() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut line = String::new();
        io::stdin().read_line(&mut line).expect("reading from io");

        let tokens = parse_cmd(line.trim());
        let all_args: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();
        if all_args.is_empty() {
            //nothing to do
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
                let mut out = redir.open_stdout_write().unwrap_or_else(|_| Box::new(io::stdout()));
                let mut err = redir.open_stderr_write().unwrap_or_else(|_| Box::new(io::stderr()));
                let _ = do_type(args, &mut *out, &mut *err);
            }
            ["pwd"] => {
                let mut out = redir.open_stdout_write().unwrap_or_else(|_| Box::new(io::stdout()));
                let mut err = redir.open_stderr_write().unwrap_or_else(|_| Box::new(io::stderr()));
                let _ = do_pwd(&mut *out, &mut *err);
            }
            ["cd", args @ ..] => {
                let mut err = redir.open_stderr_write().unwrap_or_else(|_| Box::new(io::stderr()));
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

fn parse_cmd(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut in_sq = false;
    let mut in_dq = false;

    const SQ: char = '\'';
    const DQ: char = '"';
    const BS: char = '\\';

    let mut buf = String::new();

    let mut cn: char;
    let mut it = line.chars().peekable();
    while let Some(cc) = it.next() {
        cn = ' ';
        let mut end = true;
        if let Some(&c) = it.peek() {
            cn = c;
            end = false;
        }
        if cc == BS && !in_sq && !in_dq {
            if end {
                continue;
            }
            buf.push(cn);
            let _ = it.next();
            continue;
        }
        if cc == BS && in_dq {
            if end {
                continue;
            }
            if cn == DQ || cn == BS || cn == '$' || cn == '`' || cn == '\n' {
                buf.push(cn);
                let _ = it.next();
                continue;
            }
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

fn split_redirect<'a>(args: &[&'a str]) -> (Vec<&'a str>, Redirects) {
    let mut cmd_args = Vec::new();
    let mut redir = Redirects::none();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            ">" | "1>" | ">>" | "1>>" => {
                let append = matches!(args[i], ">>" | "1>>");
                let path = args.get(i + 1).copied().unwrap_or("").to_string();
                redir.stdout = OutDest::File { append, path };
                i += 2;
            }
            "2>" | "2>>" => {
                let append = args[i] == "2>>";
                let path = args.get(i + 1).copied().unwrap_or("").to_string();
                redir.stderr = OutDest::File { append, path };
                i += 2;
            }
            "&>" | "&>>" => {
                let append = args[i] == "&>>";
                let path = args.get(i + 1).copied().unwrap_or("").to_string();
                redir.stdout = OutDest::File { append, path };
                redir.stderr = OutDest::SameAs(Stream::Stdout);
                i += 2;
            }
            "2>&1" => {
                redir.stderr = OutDest::SameAs(Stream::Stdout);
                i += 1;
            }
            "1>&2" | ">&2" => {
                redir.stdout = OutDest::SameAs(Stream::Stderr);
                i += 1;
            }
            _ => {
                cmd_args.push(args[i]);
                i += 1;
            }
        }
    }
    (cmd_args, redir)
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
    #[test]
    fn combined2_dquoe() {
        let result = parse_cmd(r#""'inside'""#);
        assert_eq!(result[0], r#"'inside'"#);
    }
    #[test]
    fn escape() {
        let result = parse_cmd(r#"\'\"literal quotes\"\'"#);
        assert_eq!(result[0], r#"'"literal"#);
        assert_eq!(result[1], r#"quotes"'"#);
    }

    #[test]
    fn backslash_in_squote() {
        let result = parse_cmd("'shell\\\nscript'");
        assert_eq!(result[0], "shell\\\nscript");
    }
    #[test]
    fn backslash_in_dquote() {
        let result = parse_cmd(r#""just'one'\\n'backslash""#);
        assert_eq!(result[0], r#"just'one'\n'backslash"#);
    }
}
